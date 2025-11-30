// Integration test suite
// @critical tests must pass before deployment

mod integration {
    mod auth_test;
    mod chat_test;
    mod error_test;
    mod health_test;
    mod metrics_test;
    mod rate_limit_test;
    mod smoke_test;
    mod test_utils;
}
