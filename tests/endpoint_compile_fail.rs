//! Compile-fail tests for typed endpoint builder invariants.

#[test]
fn endpoint_builder_requires_params() {
    let t = trybuild::TestCases::new();
    t.compile_fail("../../tests/endpoint_compile_fail/missing_params.rs");
    t.compile_fail("../../tests/endpoint_compile_fail/stringly_param.rs");
    t.compile_fail("../../tests/endpoint_compile_fail/stringly_query_pair.rs");
    t.compile_fail("../../tests/endpoint_compile_fail/stringly_query_on_ready.rs");
    t.compile_fail("../../tests/endpoint_compile_fail/post_missing_body_manual.rs");
}
