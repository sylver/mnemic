use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let dir = match args.next() {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from("docs/contract"),
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
    if let Err(err) = fs::create_dir_all(&dir) {
        eprintln!("{err}");
        return ExitCode::from(1);
    }
    if let Err(err) = fs::write(dir.join("check.schema.json"), schema)
        .and_then(|()| fs::write(dir.join("check.md"), markdown))
    {
        eprintln!("{err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
