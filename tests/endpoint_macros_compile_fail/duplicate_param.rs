use better_fetch::EndpointParamsDerive;

#[derive(EndpointParamsDerive)]
#[endpoint(path = "/items/:id")]
struct DuplicateParam {
    id: u64,
    #[param(rename = "id")]
    other: String,
}

fn main() {}
