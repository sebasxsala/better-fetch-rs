use better_fetch::EndpointParamsDerive;

#[derive(EndpointParamsDerive)]
#[endpoint(path = "/items")]
struct NoColonParams {
    id: u64,
}

fn main() {}
