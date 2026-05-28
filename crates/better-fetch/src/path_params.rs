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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_embedded_query_suffix() {
        assert_eq!(
            path_param_names("/users/:id?tab=profile"),
            vec!["id".to_string()]
        );
    }

    #[test]
    fn multiple_params_in_order() {
        assert_eq!(
            path_param_names("/orgs/:org/repos/:repo"),
            vec!["org".to_string(), "repo".to_string()]
        );
    }

    #[test]
    fn no_params_returns_empty() {
        assert!(path_param_names("/health").is_empty());
    }
}
