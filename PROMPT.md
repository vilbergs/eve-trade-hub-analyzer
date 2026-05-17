# EVE Alliance Trade Hub Analyzer — Implementation Prompt

This document is the build prompt for an autonomous coding agent. Every meaningful technology and design choice is pre-made; do not ask the user for clarification on anything specified here. If you hit a genuinely ambiguous case not covered, default to the simpler option, log the decision and rationale in a `DECISIONS.md` at the repo root, and keep moving.

Do not invent new dependencies, new tables, or new binaries beyond what this document specifies without recording the rationale in `DECISIONS.md`.

---

## 1. Goal

A Rust service that runs as a long-lived daemon on a Raspberry Pi, plus a few short-lived CLI binaries. It:

1. Polls a single player-owned EVE Online trade hub (Upwell structure) and the Jita market region via the ESI API.
2. Persists market snapshots to a managed Postgres database.
3. Produces two reports against that data: **stock health** (what's missing, low, or stale at the hub) and **seeding opportunities** (what to import from Jita for profit, ranked by expected ISK/day).

### Non-goals (do not build)

- Web UI of any kind (callback HTML page excepted).
- Multi-user platform — there is one operator, one auth character, no user accounts table.
- Multi-region or multi-hub support beyond "this hub + Jita for comparison".
- Killmail ingestion, character data ingestion (assets, wallet, skills), corporation data.
- Discord/Slack/email notifications.
- Trading API integration, automated buying/selling.
- ML/forecasting models. Heuristic-based reports only.

---

## 2. Stack

| Layer | Choice |
|---|---|
| Language | Rust, stable, edition 2024 (requires `rustc` 1.85+) |
| Async runtime | `tokio` (full feature set) |
| HTTP client | `reqwest` with `rustls-tls` (not native-tls) |
| HTTP server | `axum` — only the SSO callback route lives here |
| Database | Postgres 16, managed (Neon recommended); local docker-compose for dev |
| DB driver | `sqlx` with `postgres`, `runtime-tokio-rustls`, `macros`, `migrate`, `chrono`, `uuid` features |
| Migrations | `sqlx migrate` via `sqlx-cli` |
| Serialization | `serde`, `serde_json` |
| JWT | `jsonwebtoken` for verifying EVE SSO access tokens against the JWKS |
| OAuth | `oauth2` crate with PKCE support |
| Encryption | `aes-gcm` from RustCrypto for refresh-token-at-rest |
| Config | `dotenvy` + a serde-deserialized `Config` struct |
| CLI | `clap` 4.x with derive macros |
| Logging | `tracing` + `tracing-subscriber` with JSON formatter for prod, pretty for dev |
| Tests | `cargo test` + `wiremock` for ESI mocking |
| Tables in CLI output | `comfy-table` |
| Time handling | `chrono` with `serde` |

Do not introduce additional crates without recording the rationale.

---

(See conversation history for the full spec — sections 3–11 cover repo layout, env vars, schema, ESI endpoints, phased build plan, conventions, security checklist, and PR sequence.)
