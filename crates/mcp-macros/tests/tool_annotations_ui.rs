//! Compile-time diagnostics for malformed tool annotations.

#[test]
fn malformed_tool_annotations_are_rejected() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/tool_annotations/*.rs");
}
