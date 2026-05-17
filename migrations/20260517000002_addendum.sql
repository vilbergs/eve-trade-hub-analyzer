-- ADDENDUM.md tables: characters (plaintext refresh tokens),
-- tracked_stations, tracked_types.

CREATE TABLE characters (
    character_id      BIGINT PRIMARY KEY,
    character_name    TEXT NOT NULL,
    corporation_id    BIGINT NOT NULL,
    owner_hash        TEXT NOT NULL,
    refresh_token     TEXT NOT NULL,
    scopes            TEXT[] NOT NULL,
    status            TEXT NOT NULL CHECK (status IN ('active','needs_reauth','revoked')),
    last_refreshed_at TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE tracked_stations (
    station_id BIGINT PRIMARY KEY,
    name       TEXT
);

CREATE TABLE tracked_types (
    type_id BIGINT PRIMARY KEY REFERENCES sde_types(type_id)
);
