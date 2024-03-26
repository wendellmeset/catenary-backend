use std::collections::HashSet;

pub mod colour_correction;
pub mod convex_hull;
pub mod enum_to_int;
pub mod flatten;
pub mod rename_route_labels;
pub mod shape_colour_calculator;
pub mod stops_associated_items;

#[derive(Debug, Clone)]
pub struct DownloadAttempt {
    pub onestop_feed_id: String,
    pub file_hash: Option<String>,
    pub downloaded_unix_time_ms: i64,
    pub ingested: bool,
    pub failed: bool,
    pub mark_for_redo: bool,
    pub url: String,
    pub ingestion_version: i32,
    pub http_response_code: Option<String>,
}

pub const MAPLE_INGESTION_VERSION: i32 = 1;
