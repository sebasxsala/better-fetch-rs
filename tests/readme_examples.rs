//! Compile-check README copy-paste snippets. Keep in sync with README.md.

mod typed_endpoint_manual {
    use better_fetch::{define_params, Client, Endpoint, Result};
    use http::Method;
    use serde::Deserialize;

    define_params!(GetTodoParams for "/todos/:id" { id: u64 });

    struct GetTodo;

    impl Endpoint for GetTodo {
        const METHOD: Method = Method::GET;
        const PATH: &'static str = "/todos/:id";
        type Response = Todo;
        type Params = GetTodoParams;
        type Query = ();
        type Body = ();
        type Headers = ();
    }

    #[derive(Deserialize)]
    struct Todo {
        id: u64,
        title: String,
    }

    #[allow(dead_code)]
    async fn example(client: &Client) -> Result<()> {
        let _todo = client
            .call::<GetTodo>()
            .params(GetTodoParams { id: 1 })
            .send_json()
            .await?;
        Ok(())
    }
}

mod with_http_client {
    use better_fetch::{Client, ClientBuilder, Result};

    #[test]
    fn with_http_client_compiles() -> Result<()> {
        let reqwest = reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .build()
            .unwrap();
        let _client = Client::with_http_client(reqwest, "https://api.example.com")?;
        Ok(())
    }

    #[test]
    fn reqwest_client_builder_compiles() -> Result<()> {
        let reqwest = reqwest::Client::builder().build().unwrap();
        let _client = ClientBuilder::new()
            .reqwest_client(reqwest)
            .base_url("https://api.example.com")?
            .build()?;
        Ok(())
    }
}
