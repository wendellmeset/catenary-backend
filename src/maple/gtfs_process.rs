use std::error::Error;
use std::sync::Arc;

// Initial version 3 of ingest written by Kyler Chin
// Removal of the attribution is not allowed, as covered under the AGPL license

// take a feed id and throw it into postgres
pub async fn gtfs_process_feed(
    feed_id: &str,
    pool: &Arc<sqlx::Pool<sqlx::Postgres>>,
) -> Result<(), Box<dyn Error>> {
    let path = format!("gtfs_uncompressed/{}", feed_id);

    let gtfs = gtfs_structures::Gtfs::new(path.as_str())?;

    let (stop_ids_to_route_types, stop_ids_to_route_ids) =
        make_hashmap_stops_to_route_types_and_ids(&gtfs);

    let (stop_id_to_children_ids, stop_ids_to_children_route_types) =
        make_hashmaps_of_children_stop_info(
            &gtfs,
            &stop_ids_to_route_types,
            &stop_ids_to_route_ids,
        );

    Ok(())
}
