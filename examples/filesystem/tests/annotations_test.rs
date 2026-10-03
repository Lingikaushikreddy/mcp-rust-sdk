#[path = "../../test_support/mod.rs"]
mod support;

use serde_json::json;
use support::{assert_annotations, call, response, run_example};

#[test]
fn filesystem_reads_declare_closed_world_hints_and_preserve_contents() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "mcp-annotation-test-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).expect("test directory");
    let path = directory.join("sample.txt");
    std::fs::write(&path, "unchanged contents").expect("fixture file");
    let replies = run_example(
        env!("CARGO_BIN_EXE_mcp-example-filesystem"),
        &[directory.to_str().expect("UTF-8 path")],
        vec![
            call(3, "read_file", json!({"path": "sample.txt"})),
            call(4, "read_file", json!({"path": "sample.txt"})),
        ],
    );
    let contents = std::fs::read_to_string(&path).expect("fixture remains readable");
    std::fs::remove_dir_all(directory).expect("remove temporary fixture");
    assert_annotations(
        &replies,
        &[
            ("read_file", true),
            ("list_directory", true),
            ("search_files", true),
        ],
    );
    assert_eq!(
        response(&replies, 3)["result"]["content"][0]["text"],
        "unchanged contents"
    );
    assert_eq!(
        response(&replies, 4)["result"],
        response(&replies, 3)["result"]
    );
    assert_eq!(contents, "unchanged contents");
}
