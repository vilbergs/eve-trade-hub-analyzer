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
