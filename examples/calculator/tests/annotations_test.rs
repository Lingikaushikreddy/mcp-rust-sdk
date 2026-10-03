#[path = "../../test_support/mod.rs"]
mod support;

use serde_json::json;
use support::{assert_annotations, call, response, run_example};

#[test]
fn arithmetic_tools_declare_read_only_annotations_over_stdio() {
    let replies = run_example(
        env!("CARGO_BIN_EXE_mcp-example-calculator"),
        &[],
        vec![
            call(3, "add", json!({"a": 2, "b": 3})),
            call(4, "add", json!({"a": 2, "b": 3})),
        ],
    );
    assert_annotations(
        &replies,
        &[
            ("add", true),
            ("subtract", true),
            ("multiply", true),
            ("divide", true),
        ],
    );
    assert_eq!(response(&replies, 3)["result"]["content"][0]["text"], "5");
    assert_eq!(
        response(&replies, 4)["result"],
        response(&replies, 3)["result"]
    );
}
