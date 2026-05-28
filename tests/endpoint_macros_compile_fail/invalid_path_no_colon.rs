use better_fetch::EndpointParamsDerive;

#[derive(Default, EndpointParamsDerive)]
#[endpoint(path = "/no-colon")]
struct BadPathParams {
    id: u64,
}

fn main() {}
