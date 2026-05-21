-- Daily aggregates (snapshot-derived) and ESI history (Jita region).

CREATE TABLE market_daily_agg (
    day               DATE NOT NULL,
    location_id       BIGINT NOT NULL,
    type_id           BIGINT NOT NULL,
    open_lowest_sell  DOUBLE PRECISION,
    close_lowest_sell DOUBLE PRECISION,
    min_lowest_sell   DOUBLE PRECISION,
    max_lowest_sell   DOUBLE PRECISION,
    units_consumed    BIGINT NOT NULL DEFAULT 0,
    isk_consumed      DOUBLE PRECISION NOT NULL DEFAULT 0,
    PRIMARY KEY (day, location_id, type_id)
);
CREATE INDEX market_daily_agg_loc_type_idx ON market_daily_agg(location_id, type_id);

CREATE TABLE market_history (
    region_id   BIGINT NOT NULL,
    type_id     BIGINT NOT NULL,
    date        DATE NOT NULL,
    average     DOUBLE PRECISION NOT NULL,
    highest     DOUBLE PRECISION NOT NULL,
    lowest      DOUBLE PRECISION NOT NULL,
    volume      BIGINT NOT NULL,
    order_count BIGINT NOT NULL,
    PRIMARY KEY (region_id, type_id, date)
);
