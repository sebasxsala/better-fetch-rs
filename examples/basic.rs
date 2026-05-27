use better_fetch::{Client, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(dead_code)]
struct Todo {
    user_id: u64,
    id: u64,
    title: String,
    completed: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::new("https://jsonplaceholder.typicode.com")?;

    let todo: Todo = client
        .get("/todos/:id")
        .param("id", 1)
        .send()
        .await?
        .json()
        .await?;

    println!("{todo:#?}");
    Ok(())
}
