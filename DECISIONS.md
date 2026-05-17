# Decisions

Running log of choices that deviate from `PROMPT.md` / `ADDENDUM.md` or that resolved an ambiguity.

## 2026-05-17 — Apply ADDENDUM.md

The user issued an addendum (see `ADDENDUM.md`) that overrides several
PROMPT.md sections. This entry records the resulting rollbacks /
edits. Migrations 0001/0002 had not been applied anywhere, so the
schema files were edited in place rather than fixed forward with new
migrations (PROMPT.md §8's "don't edit committed migrations" rule
applies once they've been deployed; there is no deployed DB yet).

Rolled back or removed:

- `src/crypto.rs` (Phase 3a, prior commit) — deleted; refresh tokens
  are stored plaintext per addendum §4.
- `src/bin/discover.rs` (Phase 0 stub) — deleted; Phase 4 is skipped
  per addendum §1.
- `.github/workflows/ci.yml` (Phase 0) — deleted per addendum §4.
- Cargo deps `aes-gcm`, `rand`, `base64` — removed; no longer needed
  without `crypto.rs` or `TOKEN_ENCRYPTION_KEY`.
- Cargo `[[bin]]` entry for `discover` — removed.
- `Config` fields `hub_structure_id`, `haul_isk_per_m3`,
  `token_encryption_key` — removed. `.env.example` no longer mentions
  them.

Schema changes baked into migration `0001_sde.sql`:

- Table names rebased to the `sde_` prefix (`sde_categories`,
  `sde_groups`, `sde_market_groups`, `sde_types`, `sde_meta`).
- `sde_types` drops `packaged_volume`. `src/sde/mod.rs` no longer
  downloads `invVolumes.csv` (was added in the original Phase 1
  decision below; the addendum supersedes it).

New migration `0002_addendum.sql`:

- `characters` (plaintext `refresh_token TEXT NOT NULL`),
  `tracked_stations`, `tracked_types`.

## 2026-05-17 — Phase 5a

- **`market_orders_current.region_id` and `market_orders_snapshots.region_id` are NULLABLE.** PROMPT.md §5.4 had them NOT NULL with the implicit assumption that a single HUB_STRUCTURE_ID resolved to a region. ADDENDUM.md §2's multi-station model means we poll `/markets/structures/{id}/` for an arbitrary list of structures; ESI's structure-markets payload doesn't include region context, and looking it up per structure on every cycle would cost extra calls. Reports filter station rows by `location_id` and region rows by `region_id`, so NULL on structure rows is fine.
- **`snapshot_runs.location_id` column added.** PROMPT.md §5.5 has only `source`; under ADDENDUM.md §2 a hub cycle produces N rows (one per `tracked_stations.station_id`). `location_id` records which station / region the row is about so the operator can see "this station's poll failed."
- **`snapshot_runs.orders_kept` added next to `orders_seen`.** The whitelist filter drops most orders; recording both numbers tells you whether ESI returned data at all vs. whether the whitelist excluded everything.
- **Partitions of `market_orders_snapshots` are created at runtime, not in the migration.** Weekly partitions are time-bound; the migration sets up the parent only, and `ensure_partitions` in `src/snapshot/mod.rs` creates the current + next week's partitions on demand (and is invoked before each poll cycle).

## 2026-05-17 — Phase 3b

- **Pinned `oauth2 = "5"`, not `"4"`.** PROMPT.md §2 doesn't pin a
  version; v4 transitively pulls `reqwest 0.11` alongside our
  `reqwest 0.12`, doubling our HTTP stack. v5 supports `reqwest 0.12`
  directly and accepts our existing `reqwest::Client` for
  `request_async`.
- **No `open` crate dependency.** §7 Phase 3b step 4.iii says "via
  `open` crate (if available) or prints the URL." Printing keeps the
  dep footprint flat; the user can click the printed link.

## 2026-05-17 — Phase 2

- **Added `futures` as a dependency.** For
  `futures::stream::iter().buffer_unordered(4)` in the ESI pagination
  helper. The spec mandates "all pages concurrently up to a small
  bound (4 in flight)" and `buffer_unordered` is the idiomatic,
  well-tested primitive; the alternatives (hand-rolled `JoinSet` +
  `Semaphore`, or `chunks(4)` round-trips) are uglier and serialize
  within a chunk.

## 2026-05-17 — Phase 1 (superseded in part by ADDENDUM.md)

- **Stage every CSV into a `TEMP TABLE` of all-TEXT columns, then
  `INSERT … SELECT` with explicit casts into the real tables.**
  Fuzzwork mixes NULL markers (empty string and the literal `None`),
  and `invTypes.csv` has more columns than we keep. Doing the type
  coercion and column projection in SQL is simpler than pre-processing
  CSV bytes in Rust.
- **Version identifier is the SHA-256 of the `/dump/latest/checksum`
  body.** The checksum file already hashes every dump file, so its
  hash is a stable composite version. Added `sha2` as a dep.
- ~~Download 5 Fuzzwork CSVs, not 4.~~ Superseded by ADDENDUM.md §4:
  `sde_types` no longer has `packaged_volume`, so `invVolumes.csv` is
  no longer downloaded.

## 2026-05-17 — Phase 0

- **Crate name is `eve-trade-hub-analyzer`, not `eve-hub-analyzer`.**
  The repo directory was already created as `eve-trade-hub-analyzer`.
  Renaming the crate would require renaming the directory and the
  existing git history. Kept the existing name.
- **Added `thiserror` to dependencies.** Not listed in §2 but
  explicitly required by §8 ("Single `AppError` enum with `thiserror`
  derives").
- **Added `tempfile` as a dev-dependency.** Useful for CSV-import
  tests in Phase 1.
- **Added `url` to dependencies.** Needed for parsing the callback URL
  in the auth binary.
- **Config struct loaded by hand from `std::env`, not via
  `serde_envy`.** §2 allows "hand-written `From<EnvMap>`"; avoids an
  extra dep.
- **Binary file paths use underscores (e.g. `sde_sync.rs`) but binary
  names use hyphens (e.g. `sde-sync`).** Rust source files
  conventionally use snake_case; `[[bin]] name` is the invokable name.
- **`axum` configured with minimal features** (`tokio`, `http1`,
  `query`) — only the SSO callback route needs HTTP.
- **`reqwest` configured with `default-features = false`** plus
  `rustls-tls`, `json`, `gzip` to avoid pulling in `native-tls` /
  OpenSSL.
- **`oauth2` configured with `default-features = false`** plus
  `reqwest` and `rustls-tls` for the same reason.
