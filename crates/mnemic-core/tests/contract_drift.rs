use std::path::PathBuf;

#[test]
fn committed_contract_matches_the_types() {
    let (schema, markdown) = mnemic_core::render_contract().expect("render");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/contract");
    let committed_schema = std::fs::read_to_string(dir.join("check.schema.json")).expect("schema");
    let committed_markdown = std::fs::read_to_string(dir.join("check.md")).expect("markdown");
    assert_eq!(committed_schema, schema);
    assert_eq!(committed_markdown, markdown);
}
