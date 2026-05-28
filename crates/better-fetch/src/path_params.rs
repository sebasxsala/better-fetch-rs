//! Path template helpers shared by URL building and OpenAPI export.
//!
//! Proc-macros in `better-fetch-macros` duplicate [`path_param_names`] at compile time because
//! they cannot depend on this runtime crate.

/// Returns `:param` segment names in left-to-right order (ignores `?query` suffix).
pub fn path_param_names(path: &str) -> Vec<String> {
    let path_only = path.split_once('?').map_or(path, |(p, _)| p);
    path_only
        .split('/')
        .filter_map(|segment| segment.strip_prefix(':').map(str::to_string))
        .collect()
}
