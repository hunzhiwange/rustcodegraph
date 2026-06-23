//! Resolution module tests.
//!
//! This is the Rust port of `__tests__/resolution.test.ts`.
//!
//! Helper-level resolver tests are active. End-to-end cases that depend on the
//! TypeScript backend's full extraction, reference-resolution, caller/callee,
//! and raw SQL parity are represented as ignored tests with the original case
//! names recorded in the body.

#[path = "resolution_test/common.rs"]
mod common;

macro_rules! ignored_backend_test {
    ($name:ident, $case_name:literal) => {
        #[test]
        #[ignore = "Rust CodeGraph end-to-end extraction/query/reference backend is not at TypeScript parity yet"]
        fn $name() {
            record_backend_blocker($case_name);
        }
    };
}

#[path = "resolution_test/resolution_module.rs"]
mod resolution_module;
