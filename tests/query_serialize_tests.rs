//! Typed query serialization must surface errors (no silent omission).

use better_fetch::{endpoint::apply_serialized_query, Client, Error, Result};
use serde::Serialize;

struct BadQuery;

impl Serialize for BadQuery {
    fn serialize<S>(&self, _serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        Err(serde::ser::Error::custom("query serialize failed"))
    }
}

#[test]
fn apply_serialized_query_propagates_serialize_error() -> Result<()> {
    let client = Client::new("https://example.com")?;
    let err = apply_serialized_query(BadQuery, client.get("/search"))
        .err()
        .expect("expected query serialization to fail");

    assert!(matches!(err, Error::QuerySerialize(_)));
    Ok(())
}
