-- Per-day close-of-day stock health for every tracked (station, type):
-- lowest sell price, usable sell-side depth, and days-of-supply.
-- Populated once per day by the `rollup` binary (roll_stock_health).
CREATE TABLE IF NOT EXISTS stock_health_daily (
    day                DATE             NOT NULL,
    location_id        BIGINT           NOT NULL,
    type_id            BIGINT           NOT NULL,
    lowest_sell        DOUBLE PRECISION,
    usable_depth_units BIGINT           NOT NULL,
    days_of_supply     DOUBLE PRECISION,
    PRIMARY KEY (day, location_id, type_id)
);

CREATE INDEX IF NOT EXISTS stock_health_daily_loc_type_idx
    ON stock_health_daily (location_id, type_id);
