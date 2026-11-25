use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mnemic_core::{
    exit_code, fatal_envelope, findings_envelope, success_envelope, to_json, CheckEnvelope,
    CheckFatal, EnvelopeCode, VaultError,
};

#[derive(Parser)]
#[command(name = "mnemic", version, about = "Mnemic")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check core frontmatter in a vault or one note.
    Check {
        /// File or directory to check. Defaults to the vault root.
        path: Option<PathBuf>,
        /// Print the JSON envelope.
        #[arg(long)]
        json: bool,
        /// Vault root. Defaults to the nearest parent `mnemic.toml`.
        #[arg(long)]
        vault: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { path, json, vault } => check(path, json, vault),
    }
}

fn check(path: Option<PathBuf>, json: bool, vault: Option<PathBuf>) -> ExitCode {
    let start = path.clone().unwrap_or_else(|| PathBuf::from("."));
    let opened = mnemic_core::open_vault(&start, vault.as_deref());
    let vault = match opened {
        Ok(vault) => vault,
        Err(err) => {
            let (code, message) = fatal_from_vault(err);
            return emit(json, fatal_envelope(code, message), exit_code(code));
        }
    };
    let target = path.unwrap_or_else(|| vault.root.clone());
    match mnemic_core::check_path(&vault, &target) {
        Err(CheckFatal::Usage(message)) => emit(
            json,
            fatal_envelope(EnvelopeCode::Usage, message),
            exit_code(EnvelopeCode::Usage),
        ),
        Err(CheckFatal::Io(message)) => emit(
            json,
            fatal_envelope(EnvelopeCode::Io, message),
            exit_code(EnvelopeCode::Io),
        ),
        Ok(errors) if errors.is_empty() => emit(json, success_envelope(), 0),
        Ok(errors) => {
            let envelope = findings_envelope(errors, Vec::new());
            emit(json, envelope, exit_code(EnvelopeCode::CheckFailed))
        }
    }
}

fn fatal_from_vault(err: VaultError) -> (EnvelopeCode, String) {
    match err {
        VaultError::NotFound => (EnvelopeCode::VaultNotFound, err.to_string()),
        VaultError::BadConfig(message) => (EnvelopeCode::BadConfig, message),
        VaultError::Unsupported { found, supported } => (
            EnvelopeCode::UnsupportedVaultVersion,
            format!("vault schema_version {found} is newer than this CLI ({supported})"),
        ),
        VaultError::Io(message) => (EnvelopeCode::Io, message),
    }
}

fn emit(json: bool, envelope: CheckEnvelope, status: i32) -> ExitCode {
    let code = ExitCode::from(u8::try_from(status).unwrap_or(1));
    if json {
        match to_json(&envelope) {
            Ok(text) => print!("{text}"),
            Err(err) => {
                eprintln!("{err}");
                return ExitCode::from(1);
            }
        }
    } else if let Some(error) = &envelope.error {
        eprintln!("{}: {}", error_name(error.code), error.message);
        if let Some(data) = &envelope.data {
            for finding in &data.errors {
                eprintln!("{}: {}", finding.path, finding.message);
            }
        }
    }
    code
}

fn error_name(code: EnvelopeCode) -> &'static str {
    match code {
        EnvelopeCode::BadConfig => "BAD_CONFIG",
        EnvelopeCode::CheckFailed => "CHECK_FAILED",
        EnvelopeCode::Io => "IO",
        EnvelopeCode::LockHeld => "LOCK_HELD",
        EnvelopeCode::UnsupportedVaultVersion => "UNSUPPORTED_VAULT_VERSION",
        EnvelopeCode::Usage => "USAGE",
        EnvelopeCode::VaultNotFound => "VAULT_NOT_FOUND",
    }
}
