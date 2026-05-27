use better_fetch::{Client, Hooks, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[expect(dead_code)]
struct Todo {
    id: u64,
    title: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let hooks = Hooks::new().on_request(|ctx| async move {
        tracing::debug!(url = %ctx.url, "outgoing request");
        Ok(ctx)
    });

    let client = Client::builder()
        .base_url("https://jsonplaceholder.typicode.com")?
        .hooks(hooks)
        .build()?;

    let todo: Todo = client.get("/todos/:id").param("id", 1).send_json().await?;

    println!("{todo:#?}");
    Ok(())
}
