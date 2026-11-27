use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let root = workspace_root();
    let contract_dir = match args.next() {
        Some(path) => PathBuf::from(path),
        None => root.join("docs/contract"),
    };
    if args.next().is_some() {
        eprintln!("usage: mnemic-contract [output-dir]");
        return ExitCode::from(2);
    }
    let (schema, markdown) = match mnemic_core::render_contract() {
        Ok(rendered) => rendered,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };
    let types = match mnemic_core::render_skill_types() {
        Ok(rendered) => rendered,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };
    if let Err(err) = write_pair(
        &contract_dir,
        &[
            ("check.schema.json", schema.as_str()),
            ("check.md", markdown.as_str()),
        ],
    ) {
        eprintln!("{err}");
        return ExitCode::from(1);
    }
    let references = root.join("plugins/mnemic/skills/mnemic/references");
    if let Err(err) = write_pair(
        &references,
        &[
            ("contract.md", mnemic_core::render_skill_contract().as_str()),
            ("types.md", types.as_str()),
        ],
    ) {
        eprintln!("{err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn write_pair(dir: &Path, files: &[(&str, &str)]) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    for (name, body) in files {
        fs::write(dir.join(name), body)?;
    }
    Ok(())
}
