use std::path::{Path, PathBuf};

#[test]
fn committed_contract_matches_the_types() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (schema, markdown) = mnemic_core::render_contract().expect("render");
    let contract = root.join("docs/contract");
    assert_same(&contract.join("check.schema.json"), &schema);
    assert_same(&contract.join("check.md"), &markdown);

    let references = root.join("plugins/mnemic/skills/mnemic/references");
    assert_same(
        &references.join("contract.md"),
        &mnemic_core::render_skill_contract(),
    );
    let types = mnemic_core::render_skill_types().expect("types");
    assert_same(&references.join("types.md"), &types);
}

fn assert_same(path: &Path, expected: &str) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rel = strip_repo(&root, path);
    let committed = std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("{rel}: failed to read {}: {err}", path.display()));
    assert_eq!(committed, expected, "{rel}");
}

fn strip_repo(root: &Path, path: &Path) -> String {
    let root = normalize(root);
    let path = normalize(path);
    path.strip_prefix(&root)
        .unwrap_or(&path)
        .display()
        .to_string()
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
