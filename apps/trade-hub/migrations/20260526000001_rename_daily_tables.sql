-- Rename tables to match current code conventions, and create the
-- missing stock_health_daily table.

-- market_daily_agg → market_orders_daily
ALTER TABLE market_daily_agg RENAME TO market_orders_daily;
ALTER INDEX market_daily_agg_loc_type_idx RENAME TO market_orders_daily_loc_type_idx;

-- snapshot_runs → market_poll_runs
ALTER TABLE snapshot_runs RENAME TO market_poll_runs;
ALTER INDEX snapshot_runs_started_at_idx RENAME TO market_poll_runs_started_at_idx;
