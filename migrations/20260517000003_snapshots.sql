-- Market snapshot storage.
--
-- region_id is nullable: rows fetched from a structure-markets endpoint
-- (no region context returned by ESI) carry NULL, region rows (Jita) carry
-- the explicit region id. Reports filter by location_id for stations and by
-- region_id for region polls.
--
-- market_orders_snapshots is partitioned by week on snapshot_ts; the
-- partitions are created at runtime by ensure_partitions() (see
-- src/snapshot/mod.rs).

CREATE TABLE market_orders_current (
    order_id        BIGINT PRIMARY KEY,
    location_id     BIGINT NOT NULL,
    region_id       BIGINT,
    type_id         BIGINT NOT NULL REFERENCES sde_types(type_id),
    is_buy          BOOLEAN NOT NULL,
    price           DOUBLE PRECISION NOT NULL,
    volume_remain   BIGINT NOT NULL,
    volume_total    BIGINT NOT NULL,
    min_volume      BIGINT NOT NULL,
    range           TEXT NOT NULL,
    issued          TIMESTAMPTZ NOT NULL,
    duration_days   INT NOT NULL,
    observed_at     TIMESTAMPTZ NOT NULL
);
CREATE INDEX market_orders_current_loc_type_idx ON market_orders_current(location_id, type_id);
CREATE INDEX market_orders_current_region_type_idx
    ON market_orders_current(region_id, type_id)
    WHERE region_id IS NOT NULL;

CREATE TABLE market_orders_snapshots (
    order_id        BIGINT NOT NULL,
    snapshot_ts     TIMESTAMPTZ NOT NULL,
    location_id     BIGINT NOT NULL,
    region_id       BIGINT,
    type_id         BIGINT NOT NULL,
    is_buy          BOOLEAN NOT NULL,
    price           DOUBLE PRECISION NOT NULL,
    volume_remain   BIGINT NOT NULL,
    PRIMARY KEY (snapshot_ts, order_id)
) PARTITION BY RANGE (snapshot_ts);

CREATE TABLE snapshot_runs (
    id              BIGSERIAL PRIMARY KEY,
    started_at      TIMESTAMPTZ NOT NULL,
    finished_at     TIMESTAMPTZ,
    source          TEXT NOT NULL CHECK (source IN ('hub','jita')),
    location_id     BIGINT,
    pages_fetched   INT,
    orders_seen     INT,
    orders_kept     INT,
    error           TEXT,
    duration_ms     INT
);
CREATE INDEX snapshot_runs_started_at_idx ON snapshot_runs(started_at DESC);
