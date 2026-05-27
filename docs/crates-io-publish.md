# Publishing to crates.io

## Automated (recommended)

Push a semver tag (must match `version` in the workspace `Cargo.toml`):

```bash
git tag v0.3.0
git push origin v0.3.0
```

That runs [`.github/workflows/release.yml`](../.github/workflows/release.yml): CI checks, publish to crates.io (trusted publishing), then a GitHub Release with the matching `CHANGELOG.md` section.

### Trusted publishing setup (one-time per crate)

Configure on [crates.io](https://crates.io) → each crate → **Settings** → **Trusted Publishing**:

| Field | Value |
|-------|-------|
| GitHub owner | `sebasxsala` |
| GitHub repository | `better-fetch-rs` |
| Workflow filename | `release.yml` |
| GitHub environment | `release` |

Repeat for: `better-fetch-macros`, `better-fetch`, `typed-fetch`, `api-fetch`.

On GitHub: **Settings → Environments → New environment** → name it **`release`** (optional: require manual approval before publish).

Each crate must have been published manually at least once before trusted publishing works.

## Manual

Publish order (dependencies first):

```bash
cargo publish -p better-fetch-macros
cargo publish -p better-fetch
cargo publish -p typed-fetch
cargo publish -p api-fetch
```

`typed-fetch` and `api-fetch` re-export `better-fetch` at the same version (`0.3.0`).
