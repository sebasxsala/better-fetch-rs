use better_fetch::{Client, DefaultParamsInitial, Endpoint, EndpointBody, EndpointRequestBuilder, NeedsBody, Result};
use http::Method;
use serde::Serialize;

#[derive(Debug, Default, Serialize)]
struct TodoBody {
    title: String,
}

struct CreateTodoManual;

impl Endpoint for CreateTodoManual {
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/todos";
    type Response = ();
    type Params = ();
    type Query = ();
    type Body = TodoBody;
    type Headers = ();
}

impl EndpointBody for TodoBody {
    type ParamsNext = NeedsBody;
    type CallInitial = NeedsBody;

    fn apply_body(
        self,
        builder: better_fetch::RequestBuilder<'_>,
    ) -> Result<better_fetch::RequestBuilder<'_>> {
        builder.json(&self)
    }
}

impl DefaultParamsInitial<CreateTodoManual> for () {
    fn initial(
        client: &Client,
    ) -> EndpointRequestBuilder<'_, CreateTodoManual, NeedsBody> {
        EndpointRequestBuilder::new_needs_body(client.request(CreateTodoManual::METHOD, CreateTodoManual::PATH))
    }
}

fn main() {
    let client = Client::new("http://localhost").unwrap();
    let _ = client.call::<CreateTodoManual>().send();
}
