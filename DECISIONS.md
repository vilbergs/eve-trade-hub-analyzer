# Decisions

Running log of choices that deviate from `PROMPT.md` or that resolved an ambiguity.

## 2026-05-17 — Phase 0

- **Crate name is `eve-trade-hub-analyzer`, not `eve-hub-analyzer`.** The repo directory was already created as `eve-trade-hub-analyzer`. Renaming the crate would require renaming the directory and the existing git history. Kept the existing name.
- **Added `thiserror` to dependencies.** Not listed in §2 but explicitly required by §8 ("Single `AppError` enum with `thiserror` derives").
- **Added `rand` to dependencies.** Required by Phase 3a (`rand::rngs::OsRng` for AES-GCM nonces).
- **Added `base64` to dependencies.** Required to decode `TOKEN_ENCRYPTION_KEY` (env var is base64-encoded per §4).
- **Added `url` to dependencies.** Needed for parsing/building the OAuth callback URL; oauth2 returns `url::Url`.
- **Added `tempfile` as a dev-dependency.** Useful for CSV-import tests in Phase 1.
- **Config struct loaded by hand from `std::env`, not via `serde_envy`.** §2 allows "hand-written `From<EnvMap>`"; avoids an extra dep.
- **Binary file paths use underscores (e.g. `sde_sync.rs`) but binary names use hyphens (e.g. `sde-sync`).** Rust source files conventionally use snake_case; `[[bin]] name` is the invokable name.
- **`axum` configured with minimal features** (`tokio`, `http1`, `query`) — only the SSO callback route needs HTTP.
- **`reqwest` configured with `default-features = false`** plus `rustls-tls`, `json`, `gzip` to avoid pulling in `native-tls` / OpenSSL.
- **`oauth2` configured with `default-features = false`** plus `reqwest` and `rustls-tls` for the same reason.

## 2026-05-17 — Phase 2

- **Added `futures` as a dependency.** For `futures::stream::iter().buffer_unordered(4)` in the ESI pagination helper. The spec mandates "all pages concurrently up to a small bound (4 in flight)" and `buffer_unordered` is the idiomatic, well-tested primitive; the alternatives (hand-rolled `JoinSet` + `Semaphore`, or `chunks(4)` round-trips) are uglier and serialize within a chunk.

## 2026-05-17 — Phase 1

- **Download 5 Fuzzwork CSVs, not 4.** `invTypes.csv` does not include `packagedVolume`; that field comes from `invVolumes.csv`. The §5.2 schema requires `packaged_volume NOT NULL` and Phase 7's seeding report depends on it for haul-cost math. We download `invVolumes.csv` too and `coalesce(invVolumes.volume, invTypes.volume)` so types with no override get their unpackaged volume as a fallback. §7 Phase 1 step 2.iii says 4 CSVs; the 5th is the minimal addition that lets the schema and downstream math be honest.
- **Stage every CSV into a `TEMP TABLE` of all-TEXT columns, then `INSERT … SELECT` with explicit casts into the real tables.** Fuzzwork mixes NULL markers (empty string and the literal `None`), and `invTypes.csv` has more columns than we keep. Doing the type coercion and column projection in SQL is simpler than pre-processing CSV bytes in Rust.
- **Version identifier is the SHA-256 of the `/dump/latest/checksum` body.** The checksum file already hashes every dump file, so its hash is a stable composite version. Using `sha2` here was the cleanest option vs. storing the whole body verbatim; added as a dep.
- **Added `sha2` as a dependency.** For the version identifier above. Cheap, no native code.
