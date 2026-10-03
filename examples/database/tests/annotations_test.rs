#[path = "../../test_support/mod.rs"]
mod support;

use serde_json::json;
use support::{assert_annotations, call, response, run_example};

#[test]
fn database_hints_match_overwrite_and_repeated_mutation_effects() {
    let replies = run_example(
        env!("CARGO_BIN_EXE_mcp-example-database"),
        &[],
        vec![
            call(3, "db_set", json!({"key": "test", "value": "old"})),
            call(4, "db_set", json!({"key": "test", "value": "new"})),
            call(5, "db_get", json!({"key": "test"})),
            call(6, "db_set", json!({"key": "test", "value": "new"})),
            call(7, "db_get", json!({"key": "test"})),
            call(8, "db_delete", json!({"key": "test"})),
            call(9, "db_get", json!({"key": "test"})),
            call(10, "db_delete", json!({"key": "test"})),
            call(11, "db_get", json!({"key": "test"})),
        ],
    );
    assert_annotations(
        &replies,
        &[
            ("db_get", true),
            ("db_list", true),
            ("db_set", false),
            ("db_delete", false),
        ],
    );
    // The write replaces the old value; repeating it leaves the same value.
    assert_eq!(
        response(&replies, 5)["result"]["content"][0]["text"],
        "\"new\""
    );
    assert_eq!(
        response(&replies, 7)["result"],
        response(&replies, 5)["result"]
    );
    // Both deletions leave the key absent, even though their return text differs.
    assert_eq!(
        response(&replies, 9)["result"]["content"][0]["text"],
        "Key 'test' not found"
    );
    assert_eq!(
        response(&replies, 11)["result"],
        response(&replies, 9)["result"]
    );
}
