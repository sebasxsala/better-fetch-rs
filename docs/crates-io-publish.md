# Publishing to crates.io

Publish order (dependencies first):

```bash
cargo publish -p better-fetch-macros
cargo publish -p better-fetch
cargo publish -p typed-fetch
cargo publish -p api-fetch
```

`typed-fetch` and `api-fetch` re-export `better-fetch` at the same version (`0.2.0`).
