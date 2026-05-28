//! POST endpoints with a typed body require `.json()` / `.with_body()` before send (feature `macros`).
//!
//! ```bash
//! cargo run -p better-fetch --example needs_body --features macros,json
//! ```

use better_fetch::{Client, EndpointDerive, Result};
use http::Method;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize)]
struct CreateBody {
    title: String,
}

#[derive(Debug, Deserialize)]
struct CreateResponse {
    message: String,
}

#[derive(EndpointDerive)]
#[endpoint(method = Method::POST, path = "/todos")]
#[allow(dead_code)]
struct CreateTodo {
    #[response]
    response: CreateResponse,
    #[body]
    body: CreateBody,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::new("https://jsonplaceholder.typicode.com")?;
    let response: CreateResponse = client
        .call::<CreateTodo>()
        .json(&CreateBody {
            title: "better-fetch".into(),
        })?
        .send_json()
        .await?;
    assert!(!response.message.is_empty());
    Ok(())
}
