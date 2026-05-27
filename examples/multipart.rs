//! Multipart upload (enable `multipart` on better-fetch).
//!
//! ```bash
//! cargo run -p better-fetch --example multipart --features multipart
//! ```

use better_fetch::{Client, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::new("https://httpbin.org")?;

    let form = reqwest::multipart::Form::new()
        .text("name", "better-fetch")
        .text("version", "0.2");

    let response = client.post("/post").multipart(form).send().await?;

    println!("status: {}", response.status());
    println!("{}", response.text().await?);
    Ok(())
}
