-- Static data: EVE map + ships, populated by `intel sde-sync`.

CREATE TABLE regions (
    region_id INTEGER PRIMARY KEY,
    name      TEXT    NOT NULL
);

CREATE TABLE constellations (
    constellation_id INTEGER PRIMARY KEY,
    region_id        INTEGER NOT NULL REFERENCES regions(region_id),
    name             TEXT    NOT NULL
);

CREATE TABLE solar_systems (
    system_id        INTEGER PRIMARY KEY,
    constellation_id INTEGER NOT NULL REFERENCES constellations(constellation_id),
    name             TEXT    NOT NULL,
    security         REAL
);
CREATE INDEX solar_systems_name_lower_idx ON solar_systems(LOWER(name));
CREATE INDEX solar_systems_region_idx     ON solar_systems(constellation_id);

CREATE TABLE ship_types (
    type_id INTEGER PRIMARY KEY,
    name    TEXT    NOT NULL
);
CREATE INDEX ship_types_name_lower_idx ON ship_types(LOWER(name));

CREATE TABLE sde_meta (
    id        INTEGER PRIMARY KEY CHECK (id = 1),
    version   TEXT    NOT NULL,
    loaded_at TEXT    NOT NULL
);

-- Channel config.

CREATE TABLE channels (
    name            TEXT    PRIMARY KEY,
    filename_prefix TEXT    NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE channel_regions (
    channel_name TEXT    NOT NULL REFERENCES channels(name) ON DELETE CASCADE,
    region_id    INTEGER NOT NULL,
    PRIMARY KEY (channel_name, region_id)
);

INSERT INTO channels (name, filename_prefix) VALUES ('wc.north', 'wc.north');
INSERT INTO channel_regions (channel_name, region_id) VALUES
    ('wc.north', 10000015),   -- Venal
    ('wc.north', 10000055),   -- Branch
    ('wc.north', 10000045);   -- Tenal

-- Parsed sightings: append-only raw truth. Idempotent on (source_file, line_no).

CREATE TABLE sightings (
    id               INTEGER PRIMARY KEY,
    ts               TEXT    NOT NULL,
    channel          TEXT    NOT NULL,
    reporter         TEXT    NOT NULL,
    system_id        INTEGER REFERENCES solar_systems(system_id),
    pilots_json      TEXT    NOT NULL,
    ship_type_id     INTEGER REFERENCES ship_types(type_id),
    fleet_count      INTEGER,
    no_visual        INTEGER NOT NULL,
    is_clear         INTEGER NOT NULL,
    parse_confidence REAL    NOT NULL,
    raw_body         TEXT    NOT NULL,
    source_file      TEXT    NOT NULL,
    line_no          INTEGER NOT NULL,
    UNIQUE (source_file, line_no)
);
CREATE INDEX sightings_ts_idx        ON sightings(ts);
CREATE INDEX sightings_channel_ts_idx ON sightings(channel, ts);
CREATE INDEX sightings_system_ts_idx ON sightings(system_id, ts);

-- Per-session "I was watching this channel" windows. Bounds the denominator
-- of the safety metric.
CREATE TABLE observation_windows (
    id          INTEGER PRIMARY KEY,
    channel     TEXT    NOT NULL,
    source_file TEXT    NOT NULL UNIQUE,
    started_at  TEXT    NOT NULL,
    ended_at    TEXT    NOT NULL
);
CREATE INDEX observation_windows_channel_idx ON observation_windows(channel, started_at);

-- Derived: (system, channel) "dirty" intervals. Rebuilt by state.rs after
-- every ingest pass.
CREATE TABLE dirty_intervals (
    id         INTEGER PRIMARY KEY,
    channel    TEXT    NOT NULL,
    system_id  INTEGER NOT NULL,
    started_at TEXT    NOT NULL,
    ended_at   TEXT    NOT NULL,
    ended_by   TEXT    NOT NULL CHECK (ended_by IN ('clear', 'timeout', 'session-end'))
);
CREATE INDEX dirty_intervals_channel_sys_idx ON dirty_intervals(channel, system_id, started_at);
