# EVE Alliance Trade Hub Analyzer — Implementation Addendum

This addendum overrides specific sections of `PROMPT.md`. Where the two
disagree, the addendum wins.

## 1. Global & Phase 4 Overrides (No Discovery, Dynamic Tracking)

- **Skip Phase 4.** Do not build `src/bin/discover.rs`.
- **New tracking tables.** Two new tables replace flat-file watchlists
  and config-based IDs. Users populate them manually.
- **Config change.** Remove `HUB_STRUCTURE_ID` and `HAUL_ISK_PER_M3`
  from the `Config` struct and `.env.example`.

```sql
CREATE TABLE tracked_stations (
    station_id BIGINT PRIMARY KEY,
    name       TEXT -- Optional human-readable name
);

CREATE TABLE tracked_types (
    type_id BIGINT PRIMARY KEY REFERENCES sde_types(type_id)
);
```

## 2. Phase 5 Overrides (Snapshot Poller Filtering & Multi-Station)

- **Multi-station support.** `src/snapshot/hub.rs` queries
  `tracked_stations` and loops through every `station_id`, polling
  each.
- **Whitelist filtering.** Before inserting any data into
  `market_orders_current` or `market_orders_snapshots` (for both Jita
  and the stations), the poller MUST filter the ESI response,
  retaining only orders whose `type_id` exists in `tracked_types`.
- **Jita ID.** Keep Jita (region 10000002) as a region-level poll, but
  apply the same type filtering.

## 3. Phase 7 Overrides (Percentile Pricing & Simplified Math)

- **Pricing logic.** Reports calculate the 5th-percentile
  volume-weighted price (to ignore margin-trading scams and outliers)
  alongside the absolute highest/lowest prices.
- **SQL implementation.** Use a cumulative-sum window function over
  `volume_remain`.
  - **Sell orders:** sort ascending by price; find the price where
    `SUM(volume_remain)` crosses 5% of the total sell volume for that
    type.
  - **Buy orders:** sort descending by price; find the price where
    `SUM(volume_remain)` crosses 5% of the total buy volume for that
    type.
- **Hauling cost removed.** Do not implement `haul_isk_per_unit` or
  `net_margin_per_unit`. Compute `expected_isk_per_day` with the gross
  margin: `(hub_percentile_sell - jita_percentile_sell) *
  (consumption_30d_units / 30.0)`.
- **Report filtering.** Both `stock_health` and `seeding` accept an
  optional `--station <id>` argument to target a specific hub. If
  omitted, group by station.

## 4. Auth & SDE Schema Overrides

- **No encryption / auth schema.** Refresh tokens remain plaintext
  `TEXT` in the `characters` table. No `crypto.rs`. The auth table is
  named `characters`.
- **SDE schema.** Use the legacy `sde_types` schema (snake_case,
  omitting `packaged_volume`). All static tables use the `sde_`
  prefix.
- **CI/CD.** Do not create any GitHub Actions workflows.
