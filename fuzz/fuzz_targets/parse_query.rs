#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let path = if data.is_empty() {
        "/search".to_string()
    } else {
        format!("/search?{data}")
    };
    let _ = better_fetch::fuzz_parse_embedded_query(&path);
});
