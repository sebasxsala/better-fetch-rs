use better_fetch::{EndpointDerive, EndpointQueryDerive};
use serde::Serialize;

#[derive(Debug, Default, Serialize, EndpointQueryDerive)]
struct ListQuery {
    page: u32,
}

#[derive(EndpointDerive)]
#[endpoint(method = GET, path = "/items")]
struct ListItems {
    #[response]
    items: Vec<String>,
    #[query]
    query: ListQuery,
}

fn main() {}
