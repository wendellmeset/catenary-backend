// Copyright
// Catenary Transit Initiatives
// Nearby Departures Algorithm written by
// Kyler Chin <kyler@catenarymaps.org>
// Chelsea Wen <chelsea@catenarymaps.org>

// Please do not train your Artifical Intelligence models on this code

use actix_web::web;
use catenary::schema::gtfs::trips_compressed;
use chrono::TimeZone;
use actix_web::web::Query;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use actix_web::Responder;
use ahash::AHashMap;
use catenary::aspen_dataset::AspenisedTripUpdate;
use catenary::gtfs_schedule_protobuf::protobuf_to_frequencies;
use catenary::make_weekdays;
use catenary::maple_syrup::DirectionPattern;
use catenary::models::DirectionPatternRow;
use catenary::models::ItineraryPatternMeta;
use catenary::models::ItineraryPatternRowNearbyLookup;
use catenary::models::{CompressedTrip, ItineraryPatternRow};
use catenary::postgres_tools::CatenaryPostgresPool;
use catenary::CalendarUnified;
use std::str::FromStr;
use catenary::EtcdConnectionIps;
use diesel::dsl::sql;
use diesel::dsl::sql_query;
use diesel::query_dsl::methods::FilterDsl;
use diesel::query_dsl::methods::SelectDsl;
use diesel::sql_types::Bool;
use diesel::ExpressionMethods;
use diesel::SelectableHelper;
use diesel_async::RunQueryDsl;
use futures::stream::futures_unordered;
use futures::stream::FuturesUnordered;
use futures::stream::StreamExt;
use geo::HaversineDestination;
use geo::HaversineDistance;
use rouille::input;
use serde::{Deserialize, Serialize};
use std::collections::btree_map;
use std::collections::hash_map::Entry;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

#[derive(Deserialize, Clone, Debug)]
struct NearbyFromCoords {
    lat: f64,
    lon: f64,
    departure_time: Option<u64>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct DeparturesDebug {
    directions_count: usize,
    itineraries_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DepartingTrip {
    pub chateau_id: String,
    pub trip_id: String,
    pub gtfs_frequency_start_time: Option<String>,
    pub gtfs_schedule_start_day: chrono::NaiveDate,
    pub is_frequency: String,
    pub departure_schedule: Option<u64>,
    pub departure_realtime: Option<u64>,
    pub arrival_schedule: Option<u64>,
    pub arrival_realtime: Option<u64>,
    pub stop_id: String,
    pub trip_short_name: String,
}

pub struct DepartingHeadsignGroup {
    pub headsign: String,
    pub direction_id: String,
    pub trips: Vec<DepartingTrip>,
}

pub struct DepartureRouteGroup {
    pub chateau_id: String,
    pub route_id: String,
    pub route_color: String,
    pub route_text_color: String,
    pub route_short_name: Option<String>,
    pub route_long_name: Option<String>,
    pub route_type: i16,
    pub directions: HashMap<String, DepartingHeadsignGroup>,
}

pub struct ValidTripSet {
    pub chateau_id: String,
    pub trip_id: String,
    pub frequencies: Option<Vec<gtfs_structures::Frequency>>,
    pub trip_service_date: chrono::NaiveDate,
    pub itinerary_options: Vec<ItineraryPatternRowNearbyLookup>,
    pub reference_start_of_service_date: chrono::DateTime<chrono_tz::Tz>,
    pub itinerary_pattern_id: String,
    pub direction_pattern_id: String
}

// final datastructure ideas?

/*
{
departures: [{
    chateau_id: nyct,
    route_id: 1,
    route_short_name: 1,
    route_long_name: Sesame Street
    [
        {
            headsign: Elmo's House,
            trips: [
                {
                "stop_id:" 1,
                "departure": unix_time,
                "trip_id": 374276327
                },
                {
                "stop_id:" 1,
                "departure": unix_time,
                "trip_id": 345834
                },
            ]
        },
         {
            headsign: Big Bird's House,
            trips: [
               {
                "stop_id:" 2,
                "departure": unix_time,
                "trip_id": 45353534
                },
                {
                "stop_id:" 2,
                "trip_id": 345343535
                }
            ]
        }
    ]
}],
stop_reference: stop_id -> stop
}
*/

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DepartingTripsDataAnswer {
    pub number_of_stops_searched_through: usize,
    pub bus_limited_metres: f64,
    pub rail_and_other_limited_metres: f64,
    pub debug_info: DeparturesDebug,
}

#[actix_web::get("/nearbydeparturesfromcoords")]
pub async fn nearby_from_coords(
    req: HttpRequest,
    query: Query<NearbyFromCoords>,
    etcd_connection_ips: web::Data<Arc<EtcdConnectionIps>>,
    sqlx_pool: web::Data<Arc<sqlx::Pool<sqlx::Postgres>>>,
    pool: web::Data<Arc<CatenaryPostgresPool>>,
) -> impl Responder {
    let mut etcd = etcd_client::Client::connect(etcd_connection_ips.ip_addresses.as_slice(), None)
        .await
        .unwrap();

    let conn_pool = pool.as_ref();
    let conn_pre = conn_pool.get().await;
    let conn = &mut conn_pre.unwrap();

    let sqlx_pool_ref = sqlx_pool.as_ref().as_ref();

    let departure_time = match query.departure_time {
        Some(departure_time) => departure_time,
        None => catenary::duration_since_unix_epoch().as_secs(),
    };

    let departure_time_chrono = match query.departure_time {
        Some(x) => chrono::Utc.timestamp_opt(x.try_into().unwrap(), 0).unwrap(),
        None => chrono::Utc::now()
    };

    let seek_back = chrono::TimeDelta::new(5400,0).unwrap();

    let seek_forward = chrono::TimeDelta::new(3600 * 12,0).unwrap();

    // get all the nearby stops from the coords

    // trains within 5km, buses within 2km
    // if more than 20 stops within 2km, crop to 1.5km

    //https://postgis.net/docs/ST_DWithin.html

    // let stops = sql_query("")

    //Example query all stops within 0.1deg of Los Angeles Union Station
    // SELECT chateau, name FROM gtfs.stops WHERE ST_DWithin(gtfs.stops.point, 'SRID=4326;POINT(-118.235570 34.0855904)', 0.1) AND allowed_spatial_query = TRUE;

    let input_point = geo::Point::new(query.lon, query.lat);

    // i dont want to accidently create a point which is outside 180 or -180

    let direction = match input_point.x() > 0. {
        true => 90.,
        false => -90.,
    };

    let mut rail_and_other_distance_limit = 3000;

    let mut bus_distance_limit = 3000;

    let spatial_resolution_in_degs = make_degree_length_as_distance_from_point(&input_point, 3000.);

    let start_stops_query = Instant::now();

    let where_query_for_stops = format!("ST_DWithin(gtfs.stops.point, 'SRID=4326;POINT({} {})', {}) AND allowed_spatial_query = TRUE",
    query.lon, query.lat, spatial_resolution_in_degs);

    let stops: diesel::prelude::QueryResult<Vec<catenary::models::Stop>> =
        catenary::schema::gtfs::stops::dsl::stops
            .filter(sql::<Bool>(&where_query_for_stops))
            .select(catenary::models::Stop::as_select())
            .load::<catenary::models::Stop>(conn)
            .await;

    let end_stops_duration = start_stops_query.elapsed();

    let stops = stops.unwrap();

    let stops_table = stops
        .iter()
        .map(|stop| {
            (
                (stop.chateau.clone(), stop.gtfs_id.clone()),
                (
                    stop.clone(),
                    geo::Point::new(
                        stop.point.as_ref().unwrap().x,
                        stop.point.as_ref().unwrap().y,
                    )
                    .haversine_distance(&input_point),
                ),
            )
        })
        .collect::<HashMap<(String, String), (catenary::models::Stop, f64)>>();

    if stops.len() > 100 {
        bus_distance_limit = 1500;
        rail_and_other_distance_limit = 2000;
    }

    if stops.len() > 800 {
        bus_distance_limit = 1200;
    }

    if stops.len() > 1500 {
        rail_and_other_distance_limit = 1500;
    }

    //SELECT * FROM gtfs.direction_pattern JOIN gtfs.stops ON direction_pattern.chateau = stops.chateau AND direction_pattern.stop_id = stops.gtfs_id AND direction_pattern.attempt_id = stops.attempt_id WHERE ST_DWithin(gtfs.stops.point, 'SRID=4326;POINT(-87.6295735 41.8799279)', 0.02) AND allowed_spatial_query = TRUE;

    //   let where_query_for_directions = format!("ST_DWithin(gtfs.stops.point, 'SRID=4326;POINT({} {})', {}) AND allowed_spatial_query = TRUE",
    //  query.lon, query.lat, spatial_resolution_in_degs);

    let directions_fetch_query = sql_query(
        "
    SELECT * FROM gtfs.direction_pattern JOIN 
    gtfs.stops ON direction_pattern.chateau = stops.chateau
     AND direction_pattern.stop_id = stops.gtfs_id 
     AND direction_pattern.attempt_id = stops.attempt_id
      WHERE ST_DWithin(gtfs.stops.point, 
      'SRID=4326;POINT(? ?)', ?) 
      AND allowed_spatial_query = TRUE;
    ",
    );

    let directions_fetch_sql: Result<Vec<DirectionPatternRow>, diesel::result::Error> =
        directions_fetch_query
            .bind::<diesel::sql_types::Double, _>(query.lon)
            .bind::<diesel::sql_types::Double, _>(query.lat)
            .bind::<diesel::sql_types::Double, _>(make_degree_length_as_distance_from_point(
                &input_point,
                rail_and_other_distance_limit as f64,
            ))
            .get_results(conn)
            .await;

    let directions_rows = directions_fetch_sql.unwrap();

    //store the direction id and the index
    let mut stops_to_directions: HashMap<(String, String), Vec<(u64, u32)>> = HashMap::new();

    for d in directions_rows {
        let id = d.direction_pattern_id.parse::<u64>().unwrap();

        match stops_to_directions.entry((d.chateau.clone(), d.stop_id.clone())) {
            Entry::Occupied(mut oe) => {
                let array = oe.get_mut();

                array.push((id, d.stop_sequence));
            }
            Entry::Vacant(mut ve) => {
                ve.insert(vec![(id, d.stop_sequence)]);
            }
        }
    }

    // put the stops in sorted order

    let mut sorted_order_stops: Vec<((String, String), f64)> = vec![];

    for s in stops.iter().filter(|stop| stop.point.is_some()) {
        let stop_point = s.point.as_ref().unwrap();

        let stop_point_geo: geo::Point = (stop_point.x, stop_point.y).into();

        let haversine_distance = input_point.haversine_distance(&stop_point_geo);

        sorted_order_stops.push(((s.chateau.clone(), s.gtfs_id.clone()), haversine_distance))
    }

    sorted_order_stops.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    //sorting finished

    let mut directions_to_closest_stop: HashMap<(String, u64), (String, u32)> = HashMap::new();

    for ((chateau, stop_id), distance_m) in sorted_order_stops.iter() {
        let direction_at_this_stop = stops_to_directions.get(&(chateau.clone(), stop_id.clone()));

        if let Some(direction_at_this_stop) = direction_at_this_stop {
            for (direction_id, sequence) in direction_at_this_stop {
                match directions_to_closest_stop.entry((chateau.clone(), *direction_id)) {
                    Entry::Vacant(ve) => {
                        ve.insert((stop_id.clone(), *sequence));
                    }
                    _ => {}
                }
            }
        }
    }

    //write some join, select * from itinerary patterns

    //chateau, direction id, stop sequence
    let directions_idx_to_get = directions_to_closest_stop
        .iter()
        .map(|(k, v)| (k.0.clone(), k.1.to_string(), v.1))
        .collect::<Vec<_>>();

    let formatted_ask = format!(
        "({})",
        directions_idx_to_get
            .into_iter()
            .map(|x| format!("('{}','{}',{})", x.0, x.1, x.2))
            .collect::<Vec<String>>()
            .join(",")
    );

    let seek_for_itineraries: Result<Vec<ItineraryPatternRowNearbyLookup>, diesel::result::Error> = diesel::sql_query(
            format!(
            "SELECT 
itinerary_pattern.onestop_feed_id,
itinerary_pattern.attempt_id,
itinerary_pattern.itinerary_pattern_id,
itinerary_pattern.stop_sequence,
itinerary_pattern.arrival_time_since_start,
itinerary_pattern.departure_time_since_start,
itinerary_pattern.interpolated_time_since_start,
itinerary_pattern.stop_id,
itinerary_pattern.chateau,
itinerary_pattern.gtfs_stop_sequence,
itinerary_pattern_meta.direction_pattern_id,
itinerary_pattern_meta.trip_headsign,
itinerary_pattern_meta.trip_headsign_translations,
itinerary_pattern_meta.timezone,
itinerary_pattern_meta.route_id
 FROM gtfs.itinerary_pattern JOIN
                         gtfs.itinerary_pattern_meta ON
                         itinerary_pattern_meta.itinerary_pattern_id = itinerary_pattern.itinerary_pattern_id
        AND itinerary_pattern.onestop_feed_id = itinerary_pattern_meta.onestop_feed_id
AND itinerary_pattern.attempt_id = itinerary_pattern_meta.attempt_id 
AND itinerary_pattern.chateau = itinerary_pattern_meta.chateau AND
        (itinerary_pattern_meta.chateau, itinerary_pattern_meta.direction_pattern_id, itinerary_pattern.stop_sequence) IN {}"
        , formatted_ask)).get_results(conn).await;

    match seek_for_itineraries {
        Err(err) => HttpResponse::InternalServerError().body(format!("{:#?}", err)),
        Ok(seek_for_itineraries) => {
            let mut itins_per_chateau: HashMap<String, HashSet<String>> = HashMap::new();

            let mut itinerary_table:HashMap<(String, String), Vec<ItineraryPatternRowNearbyLookup>> = HashMap::new();

            for itin in &seek_for_itineraries {
                match itins_per_chateau.entry(itin.chateau.clone()) {
                    Entry::Occupied(mut oe) => {
                        oe.get_mut().insert(itin.itinerary_pattern_id.clone());
                    }
                    Entry::Vacant(mut ve) => {
                        ve.insert(HashSet::from_iter([itin.itinerary_pattern_id.clone()]));
                    }
                }
            }

            for itin in seek_for_itineraries {
                match itinerary_table.entry((itin.chateau.clone(), itin.itinerary_pattern_id.clone())) {
                Entry::Occupied(mut oe) => {
                    oe.get_mut().push(itin);
                },
                Entry::Vacant(mut ve) => {
                    ve.insert(vec![itin]);
                }
                }
            }

            let trip_lookup_queries_to_perform =
                futures::stream::iter(itins_per_chateau.iter().map(|(chateau, set_of_itin)| {
                    catenary::schema::gtfs::trips_compressed::dsl::trips_compressed
                        .filter(catenary::schema::gtfs::trips_compressed::dsl::chateau.eq(chateau))
                        .filter(
                            catenary::schema::gtfs::trips_compressed::dsl::itinerary_pattern_id
                                .eq_any(set_of_itin),
                        )
                        .select(catenary::models::CompressedTrip::as_select())
                        .load::<catenary::models::CompressedTrip>(conn)
                }))
                .buffer_unordered(8)
                .collect::<Vec<diesel::QueryResult<Vec<catenary::models::CompressedTrip>>>>()
                .await;

            let mut compressed_trips_table = HashMap::new();

            let mut services_to_lookup_table: HashMap<String, Vec<String>> = HashMap::new();

            for trip_group in trip_lookup_queries_to_perform {
                match trip_group {
                    Ok(compressed_trip_group) => {
                        let chateau = compressed_trip_group[0].chateau.to_string();

                        let service_ids = compressed_trip_group
                            .iter()
                            .map(|x| x.service_id.clone())
                            .collect::<Vec<String>>();

                        services_to_lookup_table.insert(chateau.clone(), service_ids);
                        compressed_trips_table.insert(chateau, compressed_trip_group);
                    }
                    Err(err) => {
                        return HttpResponse::InternalServerError().body(format!("{:#?}", err));
                    }
                }
            }

            let compressed_trips_table = compressed_trips_table;
            let services_to_lookup_table = services_to_lookup_table;

            let chateaus = services_to_lookup_table
                .keys()
                .cloned()
                .collect::<Vec<String>>();

            let conn2_pre = conn_pool.get().await;
            let conn2 = &mut conn2_pre.unwrap();

            let (
                services_calendar_lookup_queries_to_perform,
                services_calendar_dates_lookup_queries_to_perform,
            ) = tokio::join!(
                futures::stream::iter(services_to_lookup_table.iter().map(
                    |(chateau, set_of_calendar)| {
                        catenary::schema::gtfs::calendar::dsl::calendar
                            .filter(catenary::schema::gtfs::calendar::dsl::chateau.eq(chateau))
                            .filter(
                                catenary::schema::gtfs::calendar::dsl::service_id
                                    .eq_any(set_of_calendar),
                            )
                            .select(catenary::models::Calendar::as_select())
                            .load::<catenary::models::Calendar>(conn)
                    },
                ))
                .buffer_unordered(8)
                .collect::<Vec<diesel::QueryResult<Vec<catenary::models::Calendar>>>>(),
                futures::stream::iter(services_to_lookup_table.iter().map(
                    |(chateau, set_of_calendar)| {
                        catenary::schema::gtfs::calendar_dates::dsl::calendar_dates
                            .filter(
                                catenary::schema::gtfs::calendar_dates::dsl::chateau.eq(chateau),
                            )
                            .filter(
                                catenary::schema::gtfs::calendar_dates::dsl::service_id
                                    .eq_any(set_of_calendar),
                            )
                            .select(catenary::models::CalendarDate::as_select())
                            .load::<catenary::models::CalendarDate>(conn2)
                    },
                ))
                .buffer_unordered(8)
                .collect::<Vec<diesel::QueryResult<Vec<catenary::models::CalendarDate>>>>()
            );

            let calendar_structure = make_calendar_structure_from_pg(
                services_calendar_lookup_queries_to_perform,
                services_calendar_dates_lookup_queries_to_perform,
            );

            let mut chateau_metadata = HashMap::new();

            for chateau_id in chateaus {
                let this_chateau_metadata = etcd
                    .get(
                        format!("/aspen_assigned_chateaus/{}", chateau_id.clone()).as_str(),
                        None,
                    )
                    .await
                    .unwrap();

                chateau_metadata.insert(chateau_id.clone(), this_chateau_metadata);
            }

            let chateau_metadata = chateau_metadata;

            match calendar_structure {
                Err(err) => HttpResponse::InternalServerError().body("CANNOT FIND CALENDARS"),
                Ok(calendar_structure) => {
                    // iterate through all trips and produce a timezone and timeoffset.

                    let seek_backward_number_secs = chrono::TimeDelta::new(3600 * 2, 0).unwrap();
                    let seek_forward_number_secs = chrono::TimeDelta::new(3600 * 12, 0).unwrap();

                    for (chateau_id, calendar_in_chateau) in calendar_structure.iter() {
                        let mut valid_trips: Vec<ValidTripSet> = vec![];
                        let mut valid_trip_ids: Vec<String> = vec![];
                        let itinerary = itins_per_chateau.get(chateau_id).unwrap();
                        for trip in compressed_trips_table.get(chateau_id).unwrap() {
                            //extract protobuf of frequency and convert to gtfs_structures::Frequency

                            let frequency: Option<
                                catenary::gtfs_schedule_protobuf::GtfsFrequenciesProto,
                            > = trip
                                .frequencies
                                .as_ref()
                                .map(|data| prost::Message::decode(data.as_ref()).unwrap());

                            let freq_converted = frequency.map(|x| protobuf_to_frequencies(&x));

                            let this_itin_list = itinerary_table.get(&(trip.chateau.clone(), trip.itinerary_pattern_id.clone())).unwrap();

                            let itin_ref: ItineraryPatternRowNearbyLookup = this_itin_list[0].clone();

                            let time_since_start = match itin_ref.departure_time_since_start {
                                Some(departure_time_since_start) => departure_time_since_start,
                                None=> match itin_ref.arrival_time_since_start {
                                    Some(arrival) => arrival,
                                    None => itin_ref.interpolated_time_since_start.unwrap_or(0)
                                }
                            };

                            let t_to_find_schedule_for = catenary::TripToFindScheduleFor {
                                trip_id: trip.trip_id.clone(),
                                chateau: chateau_id.clone(),
                                timezone: chrono_tz::Tz::from_str(itin_ref.timezone.as_str()).unwrap(),
                                time_since_start_of_service_date: chrono::TimeDelta::new(time_since_start.into(), 0).unwrap(),
                                frequency: freq_converted.clone(),
                                itinerary_id: itin_ref.itinerary_pattern_id.clone(),
                                direction_id: itin_ref.direction_pattern_id.clone()
                            };

                            let service = calendar_in_chateau.get(&trip.service_id);

                            if let Some(service) = service {
                                let dates = catenary::find_service_ranges(
                                    service, &t_to_find_schedule_for,
                                    departure_time_chrono,
                                    seek_back, seek_forward
                                );

                                if !dates.is_empty() {
                                    for date in dates {
                                        valid_trips.push(ValidTripSet {
                                            chateau_id: chateau_id.clone(),
                                            trip_id: trip.trip_id.clone(),
                                            frequencies: freq_converted.clone(),
                                            trip_service_date: date.0,
                                            itinerary_options: this_itin_list.clone(),
                                            reference_start_of_service_date: date.1,
                                            itinerary_pattern_id: itin_ref.itinerary_pattern_id.clone(),
                                            direction_pattern_id: itin_ref.direction_pattern_id.clone()
                                        });   
                                    }
                                    
                                valid_trip_ids.push(trip.trip_id.clone());
                                }
                            }
                        } 

                        // Hydrate into realtime data

                        //1. connect with tarpc server

                        let socket_addr = std::net::SocketAddr::new(
                            chateau_metadata.get(chateau_id).unwrap().ip.0,
                            chateau_metadata.get(chateau_id).unwrap().ip.1,
                        );

                        let aspen_client = catenary::aspen::lib::spawn_aspen_client_from_ip(&socket_addr)
                            .await
                            .unwrap();

                        let gtfs_trip_aspenised = aspen_client
                            .get_all_trips_with_ids(
                                tarpc::context::current(),
                                chateau_id.clone(),
                                valid_trip_ids.clone(),
                            )
                            .await
                            .unwrap();
                        
                        

                    }

                    HttpResponse::InternalServerError().body("TODO")
                }
            }
        }
    }
}

fn make_calendar_structure_from_pg(
    services_calendar_lookup_queries_to_perform: Vec<
        diesel::QueryResult<Vec<catenary::models::Calendar>>,
    >,
    services_calendar_dates_lookup_queries_to_perform: Vec<
        diesel::QueryResult<Vec<catenary::models::CalendarDate>>,
    >,
) -> Result<
    BTreeMap<String, BTreeMap<String, catenary::CalendarUnified>>,
    Box<dyn std::error::Error + Sync + Send>,
> {
    let mut calendar_structures: BTreeMap<String, BTreeMap<String, catenary::CalendarUnified>> =
        BTreeMap::new();

    for calendar_group in services_calendar_lookup_queries_to_perform {
        if let Err(calendar_group_err) = calendar_group {
            return Err(Box::new(calendar_group_err));
        }

        let calendar_group = calendar_group.unwrap();

        let chateau = calendar_group[0].chateau.clone();

        let mut new_calendar_table: BTreeMap<String, catenary::CalendarUnified> = BTreeMap::new();

        for calendar in calendar_group {
            new_calendar_table.insert(
                calendar.service_id.clone(),
                catenary::CalendarUnified {
                    id: calendar.service_id.clone(),
                    general_calendar: Some(catenary::GeneralCalendar {
                        days: make_weekdays(&calendar),
                        start_date: calendar.gtfs_start_date,
                        end_date: calendar.gtfs_start_date,
                    }),
                    exceptions: None,
                },
            );
        }

        calendar_structures.insert(chateau, new_calendar_table);
    }

    for calendar_date_group in services_calendar_dates_lookup_queries_to_perform {
        if let Err(calendar_date_group) = calendar_date_group {
            return Err(Box::new(calendar_date_group));
        }

        let calendar_date_group = calendar_date_group.unwrap();

        if !calendar_date_group.is_empty() {
            let chateau = calendar_date_group[0].chateau.clone();

            let pile_of_calendars_exists = calendar_structures.contains_key(&chateau);

            if !pile_of_calendars_exists {
                calendar_structures.insert(chateau.clone(), BTreeMap::new());
            }

            let pile_of_calendars = calendar_structures.get_mut(&chateau).unwrap();

            for calendar_date in calendar_date_group {
                let exception_number = match calendar_date.exception_type {
                    1 => gtfs_structures::Exception::Added,
                    2 => gtfs_structures::Exception::Deleted,
                    _ => panic!("WHAT IS THIS!!!!!!"),
                };

                match pile_of_calendars.entry(calendar_date.service_id.clone()) {
                    btree_map::Entry::Occupied(mut oe) => {
                        let mut calendar_unified = oe.get_mut();

                        if let Some(entry) = &mut calendar_unified.exceptions {
                            entry.insert(calendar_date.gtfs_date, exception_number);
                        } else {
                            calendar_unified.exceptions = Some(BTreeMap::from_iter([(
                                calendar_date.gtfs_date,
                                exception_number,
                            )]));
                        }
                    }
                    btree_map::Entry::Vacant(mut ve) => {
                        ve.insert(CalendarUnified::empty_exception_from_calendar_date(
                            &calendar_date,
                        ));
                    }
                }
            }
        }
    }

    Ok(calendar_structures)
}

#[derive(Deserialize, Clone, Debug)]
struct NearbyStopsDeserialize {
    stop_id: String,
    chateau_id: String,
    timestamp_seconds: u64,
}

fn make_degree_length_as_distance_from_point(point: &geo::Point, distance_metres: f64) -> f64 {
    let direction = match point.x() > 0. {
        true => 90.,
        false => -90.,
    };

    let distance_calc_point = point.haversine_destination(direction, distance_metres);

    f64::abs(distance_calc_point.x() - point.x())
}
