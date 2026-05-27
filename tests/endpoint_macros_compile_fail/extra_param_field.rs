use better_fetch::EndpointParamsDerive;

#[derive(EndpointParamsDerive)]
#[endpoint(path = "/items/:id")]
struct ItemParams {
    id: u64,
    extra: u64,
}

fn main() {}
