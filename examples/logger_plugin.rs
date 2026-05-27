use better_fetch::{Client, LoggerPlugin, Result};
use serde::Deserialize;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Deserialize)]
#[expect(dead_code)]
struct Todo {
    id: u64,
    title: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("better_fetch=info".parse().expect("valid directive")),
        )
        .init();

    let client = Client::builder()
        .base_url("https://jsonplaceholder.typicode.com")?
        .plugin(LoggerPlugin::new().verbose(true))
        .build()?;

    let todo: Todo = client.get("/todos/:id").param("id", 1).send_json().await?;

    println!("{todo:#?}");
    Ok(())
}
