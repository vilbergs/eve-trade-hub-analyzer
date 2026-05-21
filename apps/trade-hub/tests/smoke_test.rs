// Smoke test: the library crate compiles and re-exports work.

#[test]
fn lib_re_exports_are_visible() {
    fn _takes_config(_c: eve_trade_hub_analyzer::Config) {}
    fn _takes_err(_e: eve_trade_hub_analyzer::AppError) {}
}
