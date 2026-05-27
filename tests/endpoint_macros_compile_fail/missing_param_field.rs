use better_fetch::EndpointParamsDerive;

#[derive(EndpointParamsDerive)]
#[endpoint(path = "/items/:id/:slug")]
struct ItemParams {
    id: u64,
}

fn main() {}
