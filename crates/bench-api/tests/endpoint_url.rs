//! Unit tests for `bench_api::validate_endpoint_url`.

#[test]
fn endpoint_url_validator_rejects_query_fragment_userinfo_and_malformed_hosts() {
    assert_eq!(
        bench_api::validate_endpoint_url("https://api.example.com/v1/", true),
        Some("https://api.example.com/v1".to_owned())
    );
    assert_eq!(
        bench_api::validate_endpoint_url("http://127.0.0.1:18080/api/", false),
        Some("http://127.0.0.1:18080/api/".to_owned())
    );
    assert_eq!(
        bench_api::validate_endpoint_url("http://[::1]:18080/", true),
        Some("http://[::1]:18080".to_owned())
    );
    for bad in [
        "",
        "https://",
        "https://?",
        "https://#frag",
        "https:///path",
        "https://user:pass@example.com/api",
        "https://example.com/api?capture=1",
        "https://example.com/api#frag",
        "https://example.com:0",
        "https://example.com:65536",
        "https://example.com:080",
        "https://bad_host/api",
        "https://-bad.example/api",
        "http://example.com",
        "http://127.0.0.1.evil.example:8080",
        "http://192.168.1.1:8080",
        "ftp://127.0.0.1",
    ] {
        assert_eq!(
            bench_api::validate_endpoint_url(bad, true),
            None,
            "{bad} must be rejected"
        );
    }
}
