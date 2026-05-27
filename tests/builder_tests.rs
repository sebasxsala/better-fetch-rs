use better_fetch::{ClientBuilder, Error};

#[test]
fn build_without_base_url_returns_missing_base_url() {
    let err = match ClientBuilder::new().build() {
        Ok(_) => panic!("expected MissingBaseUrl"),
        Err(e) => e,
    };
    assert!(matches!(err, Error::MissingBaseUrl));
}

#[test]
fn build_with_base_url_succeeds() {
    let client = ClientBuilder::new()
        .base_url("http://localhost")
        .unwrap()
        .build();
    assert!(client.is_ok());
}
