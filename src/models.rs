use crate::custom_pg_types::TripFrequency;
use diesel::deserialize::FromSql;
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::serialize::ToSql;
use diesel::sql_types::*;
use diesel::AsExpression;
use serde_json::Value;

#[derive(Queryable, Selectable, Insertable, Clone)]
#[diesel(table_name = crate::schema::gtfs::shapes)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Shape {
    pub onestop_feed_id: String,
    pub attempt_id: String,
    pub shape_id: String,
    pub linestring: postgis_diesel::types::LineString<postgis_diesel::types::Point>,
    pub color: Option<String>,
    pub routes: Option<Vec<Option<String>>>,
    pub route_type: i16,
    pub route_label: Option<String>,
    pub route_label_translations: Option<serde_json::Value>,
    pub text_color: Option<String>,
    pub chateau: String,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::static_download_attempts)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct StaticDownloadAttempt {
    pub onestop_feed_id: String,
    pub file_hash: Option<String>,
    pub downloaded_unix_time_ms: i64,
    pub ingested: bool,
    pub url: String,
    pub failed: bool,
    pub ingestion_version: i32,
    pub mark_for_redo: bool,
    pub http_response_code: Option<String>,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::chateaus)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Chateau {
    pub chateau: String,
    pub static_feeds: Vec<Option<String>>,
    pub realtime_feeds: Vec<Option<String>>,
    pub languages_avaliable: Vec<Option<String>>,
    pub hull: Option<postgis_diesel::types::Polygon<postgis_diesel::types::Point>>,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::static_feeds)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct StaticFeed {
    pub onestop_feed_id: String,
    pub chateau: String,
    pub previous_chateau_name: String,
    pub hull: Option<postgis_diesel::types::Polygon<postgis_diesel::types::Point>>,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::realtime_feeds)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct RealtimeFeed {
    pub onestop_feed_id: String,
    pub chateau: String,
    pub previous_chateau_name: String,
    pub fetch_interval_ms: Option<i32>,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::ingested_static)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct IngestedStatic {
    pub onestop_feed_id: String,
    pub ingest_start_unix_time_ms: i64,
    pub ingest_end_unix_time_ms: i64,
    pub ingest_duration_ms: i32,
    pub file_hash: String,
    pub attempt_id: String,
    pub ingesting_in_progress: bool,
    pub ingestion_successfully_finished: bool,
    pub ingestion_errored: bool,
    pub production: bool,
    pub deleted: bool,
    pub feed_expiration_date: Option<chrono::NaiveDate>,
    pub feed_start_date: Option<chrono::NaiveDate>,
    pub languages_avaliable: Vec<Option<String>>,
    pub ingestion_version: i32,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::in_progress_static_ingests)]
pub struct InProgressStaticIngest {
    pub onestop_feed_id: String,
    pub file_hash: String,
    pub attempt_id: String,
    pub ingest_start_unix_time_ms: i64,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::agencies)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Agency {
    pub static_onestop_id: String,
    pub agency_id: String,
    pub attempt_id: String,
    pub agency_name: String,
    pub agency_name_translations: Option<Value>,
    pub agency_url: String,
    pub agency_url_translations: Option<Value>,
    pub agency_timezone: String,
    pub agency_lang: Option<String>,
    pub agency_phone: Option<String>,
    pub agency_fare_url: Option<String>,
    pub agency_fare_url_translations: Option<Value>,
    pub chateau: String,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::trips)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Trip {
    pub onestop_feed_id: String,
    pub trip_id: String,
    pub attempt_id: String,
    pub route_id: String,
    pub service_id: String,
    pub trip_headsign: Option<String>,
    pub trip_headsign_translations: Option<Value>,
    pub has_stop_headsigns: bool,
    pub stop_headsigns: Option<Vec<Option<String>>>,
    pub trip_short_name: Option<String>,
    pub direction_id: Option<i16>,
    pub block_id: Option<String>,
    pub shape_id: Option<String>,
    pub wheelchair_accessible: Option<i16>,
    pub bikes_allowed: i16,
    pub chateau: String,
    pub frequencies: Option<Vec<Option<TripFrequencyModel>>>,
}

//Attempted custom type, still doesn't work for some reason
//Error inserting trip: SerializationError(FailedToLookupTypeError(PgMetadataCacheKey { schema: Some("public"), type_name: "trip_frequency" }))
//Even though the type clearly exists
#[derive(Clone, Debug, PartialEq, AsExpression)]
#[diesel(sql_type = crate::custom_pg_types::TripFrequency)]
pub struct TripFrequencyModel {
    pub start_time: i32,
    pub end_time: i32,
    pub headway_secs: i32,
    pub exact_times: bool,
}

use diesel::serialize::Output;
use diesel::serialize::WriteTuple;

// Learned from https://inve.rs/postgres-diesel-composite/
// https://docs.diesel.rs/2.0.x/diesel/deserialize/trait.FromSql.html
// https://docs.diesel.rs/2.0.x/diesel/serialize/trait.ToSql.html

// Imports https://docs.diesel.rs/master/diesel/pg/struct.PgValue.html as backend raw value
impl ToSql<TripFrequency, Pg> for TripFrequencyModel {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> diesel::serialize::Result {
        WriteTuple::<(Int4, Int4, Int4, Bool)>::write_tuple(
            &(
                self.start_time.clone(),
                self.end_time.clone(),
                self.headway_secs.clone(),
                self.exact_times.clone(),
            ),
            out,
        )
    }
}

impl FromSql<TripFrequency, Pg> for TripFrequencyModel {
    fn from_sql(bytes: diesel::pg::PgValue) -> diesel::deserialize::Result<Self> {
        let (start_time, end_time, headway_secs, exact_times) =
            FromSql::<Record<(Int4, Int4, Int4, Bool)>, Pg>::from_sql(bytes)?;

        Ok(TripFrequencyModel {
            start_time,
            end_time,
            headway_secs,
            exact_times,
        })
    }
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::trip_frequencies)]
pub struct TripFrequencyTableRow {
    pub onestop_feed_id: String,
    pub trip_id: String,
    pub attempt_id: String,
    pub index: i16,
    pub start_time: u32,
    pub end_time: u32,
    pub headway_secs: u32,
    pub exact_times: bool,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::stoptimes)]
pub struct StopTime {
    pub onestop_feed_id: String,
    pub attempt_id: String,
    pub trip_id: String,
    pub stop_sequence: i32,
    pub arrival_time: Option<u32>,
    pub departure_time: Option<u32>,
    pub stop_id: String,
    pub stop_headsign: Option<String>,
    pub stop_headsign_translations: Option<Value>,
    pub pickup_type: i16,
    pub drop_off_type: i16,
    pub shape_dist_traveled: Option<f32>,
    pub timepoint: bool,
    pub continuous_pickup: i16,
    pub continuous_drop_off: i16,
    pub point: Option<postgis_diesel::types::Point>,
    pub route_id: String,
    pub chateau: String,
}

/*  onestop_feed_id -> Text,
attempt_id -> Text,
gtfs_id -> Text,
name -> Text,
name_translations -> Nullable<Jsonb>,
displayname -> Text,
code -> Nullable<Text>,
gtfs_desc -> Nullable<Text>,
gtfs_desc_translations -> Nullable<Jsonb>,
location_type -> Nullable<Int2>,
parent_station -> Nullable<Text>,
zone_id -> Nullable<Text>,
url -> Nullable<Text>,
point -> Nullable<Geometry>,
timezone -> Nullable<Text>,
wheelchair_boarding -> Nullable<Int4>,
primary_route_type -> Nullable<Text>,
level_id -> Nullable<Text>,
platform_code -> Nullable<Text>,
platform_code_translations -> Nullable<Jsonb>,
routes -> Nullable<Array<Nullable<Text>>>,
route_types -> Nullable<Array<Nullable<Int2>>>,
children_ids -> Nullable<Array<Nullable<Text>>>,
children_route_types -> Nullable<Array<Nullable<Int2>>>,
station_feature -> Nullable<Bool>,
hidden -> Nullable<Bool>,
chateau -> Text,
location_alias -> Nullable<Array<Nullable<Text>>>,
tts_stop_translations -> Nullable<Jsonb>, */

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::stops)]
pub struct Stop {
    pub onestop_feed_id: String,
    pub attempt_id: String,
    pub gtfs_id: String,
    pub name: String,
    pub name_translations: Option<Value>,
    pub displayname: String,
    pub code: Option<String>,
    pub gtfs_desc: Option<String>,
    pub gtfs_desc_translations: Option<Value>,
    pub location_type: Option<i16>,
    pub parent_station: Option<String>,
    pub zone_id: Option<String>,
    pub url: Option<String>,
    pub point: Option<postgis_diesel::types::Point>,
    pub timezone: Option<String>,
    pub wheelchair_boarding: Option<i32>,
    pub primary_route_type: Option<String>,
    pub level_id: Option<String>,
    pub platform_code: Option<String>,
    pub platform_code_translations: Option<Value>,
    pub routes: Option<Vec<Option<String>>>,
    pub route_types: Option<Vec<Option<i16>>>,
    pub children_ids: Option<Vec<Option<String>>>,
    pub children_route_types: Option<Vec<Option<i16>>>,
    pub station_feature: Option<bool>,
    pub hidden: Option<bool>,
    pub chateau: String,
    pub location_alias: Option<Vec<Option<String>>>,
    pub tts_stop_translations: Option<Value>,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::calendar)]
pub struct Calendar {
    pub onestop_feed_id: String,
    pub service_id: String,
    pub monday: bool,
    pub tuesday: bool,
    pub wednesday: bool,
    pub thursday: bool,
    pub friday: bool,
    pub saturday: bool,
    pub sunday: bool,
    pub gtfs_start_date: chrono::NaiveDate,
    pub gtfs_end_date: chrono::NaiveDate,
    pub chateau: String,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::calendar_dates)]
pub struct CalendarDate {
    pub onestop_feed_id: String,
    pub attempt_id: String,
    pub service_id: String,
    pub gtfs_date: chrono::NaiveDate,
    pub exception_type: i16,
    pub chateau: String,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::gtfs::feed_info)]
pub struct FeedInfo {
    pub onestop_feed_id: String,
    pub feed_publisher_name: String,
    pub feed_publisher_url: String,
    pub feed_lang: Option<String>,
    pub feed_start_date: Option<chrono::NaiveDate>,
    pub feed_end_date: Option<chrono::NaiveDate>,
    pub feed_version: Option<String>,
    pub chateau: String,
}
