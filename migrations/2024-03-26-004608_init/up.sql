-- Your SQL goes here
-- Add migration script here
-- Initial version 3 of ingest: Kyler Chin
-- This was heavily inspired and copied from Emma Alexia, thank you Emma!

CREATE EXTENSION IF NOT EXISTS postgis;
CREATE SCHEMA IF NOT EXISTS gtfs;
CREATE EXTENSION IF NOT EXISTS hstore;

CREATE TABLE IF NOT EXISTS gtfs.static_download_attempts (
   onestop_feed_id text NOT NULL,
   file_hash text,
   downloaded_unix_time_ms bigint NOT NULL,
   ingested boolean NOT NULL,
   url text NOT NULL,
   failed boolean NOT NULL,
   ingestion_version integer NOT NULL,
   mark_for_redo boolean NOT NULL,
   http_response_code text,
   PRIMARY KEY (onestop_feed_id, downloaded_unix_time_ms)
);

CREATE TABLE gtfs.ingested_static (
    onestop_feed_id text NOT NULL,
    -- hash of the zip file
    file_hash text NOT NULL,
    attempt_id text NOT NULL,
    ingest_start_unix_time_ms bigint NOT NULL,
    ingesting_in_progress boolean NOT NULL,
    ingestion_successfully_finished boolean NOT NULL,
    ingestion_errored boolean NOT NULL,
    production boolean NOT NULL,
    deleted boolean NOT NULL,
    feed_expiration_date date,
    feed_start_date date,
    languages_avaliable text[] NOT NULL,
    ingestion_version integer NOT NULL,
    PRIMARY KEY (onestop_feed_id, ingest_start_unix_time_ms)
);

CREATE INDEX IF NOT EXISTS gtfs_static_download_attempts_file_hash ON gtfs.static_download_attempts (file_hash);

CREATE TABLE gtfs.static_feeds (
    onestop_feed_id text NOT NULL PRIMARY KEY,
    chateau text NOT NULL,
    previous_chateau_name text NOT NULL,
    hull GEOMETRY(POLYGON,4326)
);

CREATE INDEX static_hulls ON gtfs.static_feeds USING GIST (hull);

CREATE TABLE gtfs.chateaus (
    chateau text NOT NULL PRIMARY KEY,
    static_feeds text[] NOT NULL,
    realtime_feeds text[] NOT NULL,
    languages_avaliable text[] NOT NULL,
    hull GEOMETRY(POLYGON,4326)
);

-- this dataset may be missing
-- if the feed start end date or end date is missing, replace the file
-- switch data asap ASAP if the start date is before the current date
-- time enable of new data when the current feed expires
CREATE TABLE gtfs.feed_info (
    onestop_feed_id text,
    feed_publisher_name text,
    feed_publisher_url text,
    feed_lang text,
    default_lang text,
    feed_start_date DATE,
    feed_end_date DATE,
    feed_version text,
    feed_contact_email text,
    feed_contact_url text,
    attempt_id text,
    chateau text,
    PRIMARY KEY (onestop_feed_id, attempt_id, feed_publisher_name)
);

CREATE INDEX IF NOT EXISTS chateau_feed_info ON gtfs.feed_info (chateau);

--CREATE TABLE gtfs.operators (
--    onestop_operator_id text PRIMARY KEY,
--    name text,
--    gtfs_static_feeds text[],
--    gtfs_realtime_feeds text[],
--    static_onestop_feeds_to_gtfs_ids jsonb,
--    realtime_onestop_feeds_to_gtfs_ids hstore
--);

CREATE TABLE gtfs.realtime_feeds (
    onestop_feed_id text PRIMARY KEY,
    name text,
    -- operators text[],
    -- operators_to_gtfs_ids jsonb,
    --max_lat double precision,
    --max_lon double precision,
    --min_lat double precision,
    --min_lon double precision,
    previous_chateau_name text NOT NULL,
    chateau text NOT NULL,
    fetch_interval_ms integer
);

CREATE TABLE gtfs.agencies (
    static_onestop_id text NOT NULL,
    -- Option<String> where None is a valid key
    agency_id text,
    attempt_id text NOT NULL,
    agency_name text NOT NULL,
    agency_name_translations jsonb,
    agency_url text NOT NULL,
    agency_url_translations jsonb,
    agency_timezone text NOT NULL,
    agency_lang text,
    agency_phone text,
    agency_fare_url	text,
    agency_fare_url_translations jsonb,
    chateau text NOT NULL,
    PRIMARY KEY (static_onestop_id, attempt_id, agency_id)
);

CREATE INDEX IF NOT EXISTS agencies_chateau ON gtfs.agencies (chateau);

CREATE TABLE gtfs.routes (
    onestop_feed_id text NOT NULL,
    attempt_id text NOT NULL,
    route_id text NOT NULL,
    short_name text NOT NULL,
    short_name_translations jsonb,
    long_name text NOT NULL,
    long_name_translations jsonb,
    gtfs_desc text,
    route_type smallint NOT NULL,
    url text,
    url_translations jsonb,
    agency_id text,
    gtfs_order int,
    color text,
    text_color text,
    continuous_pickup smallint,
    continuous_drop_off smallint,
    shapes_list text[],
    chateau text NOT NULL,
    PRIMARY KEY (onestop_feed_id, attempt_id, route_id)
);

CREATE INDEX gtfs_routes_chateau_index ON gtfs.routes (chateau);
CREATE INDEX gtfs_routes_type_index ON gtfs.routes (route_type);

CREATE TABLE IF NOT EXISTS gtfs.shapes (
    onestop_feed_id text NOT NULL,
    attempt_id text NOT NULL,
    shape_id text NOT NULL,
    linestring geometry(Linestring,4326) NOT NULL,
    color text,
    routes text[],
    route_type smallint NOT NULL,
    route_label text,
    route_label_translations jsonb,
    text_color text,
    chateau text NOT NULL,
    PRIMARY KEY (onestop_feed_id, attempt_id, shape_id)
);

CREATE INDEX IF NOT EXISTS shapes_chateau ON gtfs.shapes (chateau);
CREATE INDEX shapes_linestring_index ON gtfs.shapes USING GIST (linestring);

-- no nulls so just contrain and unwrap ngl
CREATE TYPE gtfs.trip_frequency_pre AS (
    start_time integer,
    end_time integer,
    headway_secs integer,
    -- false is zero [frequency based trips], true is schedule based
    -- None should return false
    exact_times boolean
);

create domain gtfs.trip_frequency as trip_frequency_pre
check (
  (value).start_time is not null and 
  (value).start_time >=0 and 
  (value).end_time is not null and
  (value).end_time >= 0 and
  (value).headway_secs is not null and
  (value).headway_secs >= 0 and
  (value).exact_times is not null
);

CREATE TABLE gtfs.trips (
    trip_id text NOT NULL,
    onestop_feed_id text NOT NULL,
    attempt_id text NOT NULL,
    route_id text NOT NULL,
    service_id text NOT NULL,
    trip_headsign text,
    trip_headsign_translations jsonb,
    has_stop_headsigns boolean NOT NULL,
    stop_headsigns text[],
    trip_short_name text,
    direction_id smallint,
    block_id text,
    shape_id text,
    wheelchair_accessible smallint,
    bikes_allowed smallint NOT NULL,
    chateau text NOT NULL,
    frequencies gtfs.trip_frequency[],
    PRIMARY KEY (onestop_feed_id, attempt_id, trip_id)
);

CREATE TABLE gtfs.f_test (
trip_id text NOT NULL PRIMARY KEY,
f trip_frequency[]
);

CREATE INDEX IF NOT EXISTS trips_chateau ON gtfs.trips (chateau);

CREATE TABLE gtfs.stops (
    onestop_feed_id text NOT NULL,
    attempt_id text NOT NULL,
    gtfs_id text NOT NULL,
    name text NOT NULL,
    name_translations jsonb,
    displayname text NOT NULL,
    code text,
    gtfs_desc text,
    gtfs_desc_translations jsonb,
    location_type smallint,
    parent_station text,
    zone_id text,
    url text,
    point GEOMETRY(POINT, 4326) NOT NULL,
    timezone text,
    wheelchair_boarding int,
    primary_route_type text,
    level_id text,
    platform_code text,
    platform_code_translations jsonb,
    routes text[],
    route_types smallint[],
    children_ids text[],
    children_route_types smallint[],
    station_feature boolean,
    hidden boolean,
    chateau text NOT NULL,
    location_alias text[],
    tts_stop_translations jsonb,
    PRIMARY KEY (onestop_feed_id, attempt_id, gtfs_id)
);

CREATE INDEX gtfs_static_stops_geom_idx ON gtfs.stops USING GIST (point);
CREATE INDEX stops_chateau_idx ON gtfs.stops (chateau);

CREATE TABLE gtfs.stoptimes (
    onestop_feed_id text NOT NULL,
    attempt_id text NOT NULL,
    trip_id text NOT NULL,
    stop_sequence int NOT NULL,
    arrival_time bigint,
    departure_time bigint,
    stop_id text NOT NULL,
    stop_headsign text,
    stop_headsign_translations jsonb,
    pickup_type int,
    drop_off_type int,
    shape_dist_traveled double precision,
    timepoint int,
    continuous_pickup smallint,
    continuous_drop_off smallint,
    point GEOMETRY(POINT, 4326) NOT NULL,
    route_id text,
    chateau text NOT NULL,
    PRIMARY KEY (onestop_feed_id, attempt_id, trip_id, stop_sequence)
);

CREATE TABLE gtfs.gtfs_errors (
onestop_feed_id text NOT NULL,
error text NOT NULL,
attempt_id text,
file_hash text,
chateau text NOT NULL,
PRIMARY KEY (onestop_feed_id, attempt_id)
);

CREATE TABLE gtfs.realtime_passwords (
    onestop_feed_id text NOT NULL PRIMARY KEY,
    passwords text[],
    header_auth_key text,
    header_auth_value_prefix text,
    url_auth_key text
);

CREATE TABLE gtfs.static_passwords (
    onestop_feed_id text NOT NULL PRIMARY KEY,
    passwords text[],
    header_auth_key text,
    header_auth_value_prefix text,
    url_auth_key text
);

CREATE TABLE gtfs.calendar_dates (
    onestop_feed_id text NOT NULL,
    attempt_id text NOT NULL,
    service_id text NOT NULL,
    gtfs_date date NOT NULL,
    exception_type smallint NOT NULL,
    PRIMARY KEY (onestop_feed_id, service_id, gtfs_date)
);

CREATE TABLE gtfs.calendar (
    onestop_feed_id text NOT NULL,
    attempt_id text NOT NULL,
    service_id text NOT NULL,
    monday boolean NOT NULL,
    tuesday boolean NOT NULL,
    wednesday boolean NOT NULL,
    thursday boolean NOT NULL,
    friday boolean NOT NULL,
    saturday boolean NOT NULL,
    sunday boolean NOT NULL,
    gtfs_start_date date NOT NULL,
    gtfs_end_date date NOT NULL,
    PRIMARY KEY (onestop_feed_id, attempt_id, service_id)
);

-- translations does not need a table, values should be directly inserted into the data structure