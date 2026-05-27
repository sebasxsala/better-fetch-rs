use better_fetch::{Auth, Client, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[expect(dead_code)]
struct Post {
    id: u64,
    title: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // jsonplaceholder ignores auth; this demonstrates wiring Bearer tokens.
    let client = Client::builder()
        .base_url("https://jsonplaceholder.typicode.com")?
        .auth(Auth::bearer("example-token"))
        .build()?;

    let post: Post = client.get("/posts/:id").param("id", 1).send_json().await?;

    println!("{post:#?}");
    Ok(())
}
