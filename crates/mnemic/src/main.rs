use std::path::{Path, PathBuf};
use std::process::ExitCode;

use std::collections::BTreeMap;

use clap::Parser;
use mnemic::cli::{Cli, Command};
use mnemic_core::{
    check_fix, create_note, exit_code, fatal_envelope, fatal_envelope_with, findings_envelope,
    fix_envelope, init_vault, log_note, move_note, read_note, search_vault, success_envelope,
    to_json, CheckEnvelope, CheckFatal, EnvelopeCode, FixError, FixRequest, InitData, InitError,
    InitRequest, InitSuccess, LogError, LogRequest, MoveError, MoveRequest, NewError, NewRequest,
    NoteView, ReadError, SearchData, SearchEnvelope, SearchError, SearchRequest, SystemClock,
    UlidGenerator, VaultError,
};
use serde::Serialize;
use serde_json::{json, Value};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Check {
            path,
            json,
            vault,
            fix,
        } => check(path, json, vault, fix),
        Command::Init { json, vault, actor } => init(json, vault, actor),
        Command::Log {
            note,
            text,
            json,
            vault,
        } => log(note, text, json, vault),
        Command::Search {
            query,
            type_name,
            tag,
            since,
            limit,
            json,
            vault,
        } => search(query, type_name, tag, since, limit, json, vault),
        Command::Get { query, json, vault } => show(query, json, vault, false),
        Command::Links { note, json, vault } => show(note, json, vault, true),
        Command::New {
            type_name,
            title,
            id,
            actor,
            date,
            path,
            url,
            json,
            vault,
        } => new_note(NewArgs {
            type_name,
            title,
            id,
            actor,
            date,
            path,
            url,
            json,
            vault,
        }),
        Command::Mv {
            note,
            dest,
            no_rewrite,
            json,
            vault,
        } => mv(note, dest, no_rewrite, json, vault),
        Command::Skill { reference, json } => skill(reference, json),
    }
}

const SKILL_MD: &str = include_str!("../../../plugins/mnemic/skills/mnemic/SKILL.md");
const SKILL_CONTRACT: &str =
    include_str!("../../../plugins/mnemic/skills/mnemic/references/contract.md");
const SKILL_TYPES: &str = include_str!("../../../plugins/mnemic/skills/mnemic/references/types.md");

fn skill(reference: Option<String>, json: bool) -> ExitCode {
    let (name, content) = match reference.as_deref() {
        None => ("SKILL.md", SKILL_MD),
        Some("contract") => ("references/contract.md", SKILL_CONTRACT),
        Some("types") => ("references/types.md", SKILL_TYPES),
        Some(other) => {
            return emit(
                json,
                fatal_envelope(EnvelopeCode::Usage, format!("unknown reference: {other}")),
                exit_code(EnvelopeCode::Usage),
            );
        }
    };
    if json {
        let value = SkillEnvelope {
            ok: true,
            data: Some(SkillData {
                name,
                version: env!("CARGO_PKG_VERSION"),
                content,
            }),
            error: None,
        };
        match serde_json::to_string_pretty(&value) {
            Ok(text) => println!("{text}"),
            Err(err) => {
                eprintln!("{err}");
                return ExitCode::from(1);
            }
        }
        ExitCode::SUCCESS
    } else {
        print!("{content}");
        ExitCode::SUCCESS
    }
}

fn mv(
    note: String,
    dest: PathBuf,
    no_rewrite: bool,
    json: bool,
    vault: Option<PathBuf>,
) -> ExitCode {
    let root = match opened_vault(vault.as_deref(), None) {
        Ok(vault) => vault.root,
        Err((code, message)) => {
            return emit(json, fatal_envelope(code, message), exit_code(code));
        }
    };
    let dest = dest.to_string_lossy();
    let result = move_note(MoveRequest {
        vault: &root,
        note: &note,
        dest: &dest,
        rewrite: !no_rewrite,
        clock: &SystemClock,
        lock_timeout: mnemic_core::write::DEFAULT_LOCK_TIMEOUT,
    });
    match result {
        Ok(result) => {
            if json {
                match serde_json::to_string_pretty(&result.success()) {
                    Ok(text) => println!("{text}"),
                    Err(err) => {
                        eprintln!("{err}");
                        return ExitCode::from(1);
                    }
                }
            } else {
                println!("moved {} {}", result.from, result.to);
                for path in &result.rewritten {
                    println!("rewrote {path}");
                }
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            let (code, message, details) = fatal_from_mv(err);
            let envelope = if details.is_empty() {
                fatal_envelope(code, message)
            } else {
                fatal_envelope_with(code, message, details)
            };
            emit(json, envelope, exit_code(code))
        }
    }
}

fn fatal_from_mv(err: MoveError) -> (EnvelopeCode, String, BTreeMap<String, Value>) {
    match err {
        MoveError::Usage(message) => (EnvelopeCode::Usage, message, BTreeMap::new()),
        MoveError::NotFound(message) => (EnvelopeCode::NotFound, message, BTreeMap::new()),
        MoveError::Ambiguous { candidates } => {
            let rows: Vec<Value> = candidates
                .iter()
                .map(|path| json!({ "path": path }))
                .collect();
            let mut details = BTreeMap::new();
            details.insert("candidates".to_string(), Value::Array(rows));
            (
                EnvelopeCode::Ambiguous,
                format!("note matches more than one path: {}", candidates.join(", ")),
                details,
            )
        }
        MoveError::OutsideVault(message) => (EnvelopeCode::OutsideVault, message, BTreeMap::new()),
        MoveError::DestExists(message) => (EnvelopeCode::DestExists, message, BTreeMap::new()),
        MoveError::NoteArchived(message) => (EnvelopeCode::NoteArchived, message, BTreeMap::new()),
        MoveError::AliasNotUnique(message) => {
            (EnvelopeCode::AliasNotUnique, message, BTreeMap::new())
        }
        MoveError::SlugCollision(message) => {
            (EnvelopeCode::SlugCollision, message, BTreeMap::new())
        }
        MoveError::RawImmutable(message) => (EnvelopeCode::RawImmutable, message, BTreeMap::new()),
        MoveError::LockHeld { .. } => (EnvelopeCode::LockHeld, err.to_string(), BTreeMap::new()),
        MoveError::Io(message) => (EnvelopeCode::Io, message, BTreeMap::new()),
        MoveError::Vault(message) => {
            let (code, message) = fatal_from_vault_message(&message);
            (code, message, BTreeMap::new())
        }
    }
}

struct NewArgs {
    type_name: String,
    title: String,
    id: Option<String>,
    actor: Option<String>,
    date: Option<String>,
    path: Option<String>,
    url: Option<String>,
    json: bool,
    vault: Option<PathBuf>,
}

fn new_note(args: NewArgs) -> ExitCode {
    let NewArgs {
        type_name,
        title,
        id,
        actor,
        date,
        path,
        url,
        json,
        vault,
    } = args;
    let root = match resolve_root(vault.as_deref(), None) {
        Ok(root) => root,
        Err((code, message)) => {
            return emit(json, fatal_envelope(code, message), exit_code(code));
        }
    };
    let mut ids = UlidGenerator::from_seed(random_seed());
    let result = create_note(NewRequest {
        vault: &root,
        type_name: &type_name,
        title: &title,
        id: id.as_deref(),
        actor: actor.as_deref(),
        date: date.as_deref(),
        path: path.as_deref(),
        url: url.as_deref(),
        clock: &SystemClock,
        ids: &mut ids,
        lock_timeout: mnemic_core::write::DEFAULT_LOCK_TIMEOUT,
    });
    match result {
        Ok(result) => {
            if json {
                match serde_json::to_string_pretty(&result.success()) {
                    Ok(text) => println!("{text}"),
                    Err(err) => {
                        eprintln!("{err}");
                        return ExitCode::from(1);
                    }
                }
            } else if result.noop {
                println!("noop {} {}", result.path, result.id);
            } else {
                println!("{} {}", result.path, result.id);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            let (code, message) = fatal_from_new(err);
            emit(json, fatal_envelope(code, message), exit_code(code))
        }
    }
}

fn fatal_from_new(err: NewError) -> (EnvelopeCode, String) {
    match err {
        NewError::Usage(message) => (EnvelopeCode::Usage, message),
        NewError::UnknownType(message) => (EnvelopeCode::UnknownType, message),
        NewError::BadId(message) => (EnvelopeCode::BadId, message),
        NewError::IdExists(message) => (EnvelopeCode::IdExists, message),
        NewError::SlugCollision(message) => (EnvelopeCode::SlugCollision, message),
        NewError::MissingTemplate(message) => (EnvelopeCode::MissingTemplate, message),
        NewError::OutsideVault(message) => (EnvelopeCode::OutsideVault, message),
        NewError::LockHeld { .. } => (EnvelopeCode::LockHeld, err.to_string()),
        NewError::Io(message) => (EnvelopeCode::Io, message),
        NewError::Vault(message) => fatal_from_vault_message(&message),
    }
}

fn random_seed() -> u64 {
    mnemic_core::write::random_u64().unwrap_or_else(|_| u64::from(std::process::id()))
}

fn log(note: String, text: String, json: bool, vault: Option<PathBuf>) -> ExitCode {
    let root = match opened_vault(vault.as_deref(), None) {
        Ok(vault) => vault.root,
        Err((code, message)) => {
            return emit(json, fatal_envelope(code, message), exit_code(code));
        }
    };
    let result = log_note(LogRequest {
        vault: &root,
        note: &note,
        text: &text,
        clock: &SystemClock,
        lock_timeout: mnemic_core::write::DEFAULT_LOCK_TIMEOUT,
    });
    match result {
        Ok(result) => {
            if json {
                match serde_json::to_string_pretty(&result.success()) {
                    Ok(text) => println!("{text}"),
                    Err(err) => {
                        eprintln!("{err}");
                        return ExitCode::from(1);
                    }
                }
            } else if result.noop {
                println!("noop {}", result.line);
            } else if result.heading_created {
                println!("created heading {}", result.line);
            } else {
                println!("{}", result.line);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            let (code, message) = fatal_from_log(err);
            emit(json, fatal_envelope(code, message), exit_code(code))
        }
    }
}

fn fatal_from_log(err: LogError) -> (EnvelopeCode, String) {
    match err {
        LogError::Usage(message) => (EnvelopeCode::Usage, message),
        LogError::NotFound(message) => (EnvelopeCode::NotFound, message),
        LogError::Ambiguous { .. } => (EnvelopeCode::Ambiguous, err.to_string()),
        LogError::TimelineDisabled(message) => (EnvelopeCode::TimelineDisabled, message),
        LogError::TimelineHeadingAmbiguous(message) => {
            (EnvelopeCode::TimelineHeadingAmbiguous, message)
        }
        LogError::OutsideVault(message) => (EnvelopeCode::OutsideVault, message),
        LogError::LockHeld { .. } => (EnvelopeCode::LockHeld, err.to_string()),
        LogError::Io(message) => (EnvelopeCode::Io, message),
        LogError::Vault(message) => fatal_from_vault_message(&message),
    }
}

fn fatal_from_vault_message(message: &str) -> (EnvelopeCode, String) {
    if message.contains("no mnemic.toml") {
        (EnvelopeCode::VaultNotFound, message.to_string())
    } else if message.contains("schema_version") && message.contains("newer") {
        (EnvelopeCode::UnsupportedVaultVersion, message.to_string())
    } else {
        (EnvelopeCode::BadConfig, message.to_string())
    }
}

fn search(
    query: Option<String>,
    type_name: Option<String>,
    tag: Option<String>,
    since: Option<String>,
    limit: Option<i64>,
    json: bool,
    vault: Option<PathBuf>,
) -> ExitCode {
    let Some(query) = query.filter(|text| !text.trim().is_empty()) else {
        return emit(
            json,
            fatal_envelope(EnvelopeCode::Usage, "query is empty"),
            exit_code(EnvelopeCode::Usage),
        );
    };
    let limit = match limit {
        None => 20,
        Some(value) if value > 0 => value as usize,
        Some(_) => {
            return emit(
                json,
                fatal_envelope(EnvelopeCode::Usage, "limit must be greater than zero"),
                exit_code(EnvelopeCode::Usage),
            );
        }
    };
    let root = match opened_vault(vault.as_deref(), None) {
        Ok(vault) => vault.root,
        Err((code, message)) => {
            return emit(json, fatal_envelope(code, message), exit_code(code));
        }
    };
    match search_vault(
        &root,
        SearchRequest {
            query: &query,
            type_name: type_name.as_deref(),
            tag: tag.as_deref(),
            since: since.as_deref(),
            limit,
        },
    ) {
        Ok(data) => emit_search(json, data),
        Err(SearchError::Usage(message)) => emit(
            json,
            fatal_envelope(EnvelopeCode::Usage, message),
            exit_code(EnvelopeCode::Usage),
        ),
        Err(SearchError::Io(message)) => emit(
            json,
            fatal_envelope(EnvelopeCode::Io, message),
            exit_code(EnvelopeCode::Io),
        ),
    }
}

fn emit_search(json: bool, data: SearchData) -> ExitCode {
    if json {
        let envelope = SearchEnvelope {
            ok: true,
            data: Some(data),
            error: None,
        };
        match serde_json::to_string_pretty(&envelope) {
            Ok(text) => println!("{text}"),
            Err(err) => {
                eprintln!("{err}");
                return ExitCode::from(1);
            }
        }
    } else {
        for hit in &data.hits {
            println!("{}\t{}\t{}", hit.score, hit.id, hit.path);
        }
    }
    ExitCode::SUCCESS
}

fn show(query: String, json: bool, vault: Option<PathBuf>, links_only: bool) -> ExitCode {
    let root = match opened_vault(vault.as_deref(), None) {
        Ok(vault) => vault.root,
        Err((code, message)) => {
            return emit(json, fatal_envelope(code, message), exit_code(code));
        }
    };
    match read_note(&root, &query) {
        Ok(note) => emit_note(json, &note, links_only),
        Err(err) => {
            let (code, envelope) = fatal_from_read(err);
            emit(json, envelope, exit_code(code))
        }
    }
}

fn fatal_from_read(err: ReadError) -> (EnvelopeCode, CheckEnvelope) {
    match err {
        ReadError::NotFound(message) => (
            EnvelopeCode::NotFound,
            fatal_envelope(EnvelopeCode::NotFound, format!("no note matches {message}")),
        ),
        ReadError::Ambiguous { query, candidates } => {
            let rows: Vec<Value> = candidates
                .iter()
                .map(|candidate| json!({ "id": candidate.id, "path": candidate.path }))
                .collect();
            let mut details = BTreeMap::new();
            details.insert("candidates".to_string(), Value::Array(rows));
            let paths: Vec<&str> = candidates
                .iter()
                .map(|candidate| candidate.path.as_str())
                .collect();
            (
                EnvelopeCode::Ambiguous,
                fatal_envelope_with(
                    EnvelopeCode::Ambiguous,
                    format!("{query} matches {}", paths.join(", ")),
                    details,
                ),
            )
        }
        ReadError::OutsideVault(message) => (
            EnvelopeCode::OutsideVault,
            fatal_envelope(
                EnvelopeCode::OutsideVault,
                format!("{message} is outside the vault"),
            ),
        ),
        ReadError::Io(message) => (EnvelopeCode::Io, fatal_envelope(EnvelopeCode::Io, message)),
    }
}

fn emit_note(json: bool, note: &NoteView, links_only: bool) -> ExitCode {
    if json {
        let value = if links_only {
            json!({
                "ok": true,
                "data": {
                    "id": note.id,
                    "path": note.path,
                    "outgoing": note.outgoing,
                    "backlinks": note.backlinks,
                    "broken": note.broken,
                },
                "error": null,
            })
        } else {
            json!({
                "ok": true,
                "data": {
                    "id": note.id,
                    "path": note.path,
                    "slug": note.slug,
                    "title": note.title,
                    "type": note.type_name,
                    "status": note.status,
                    "frontmatter": note.frontmatter,
                    "body": note.body,
                    "outgoing": note.outgoing,
                    "backlinks": note.backlinks,
                },
                "error": null,
            })
        };
        return print_value(value);
    }
    if links_only {
        println!("{} {}", note.id, note.path);
        print_edges("outgoing", &note.outgoing);
        print_edges("backlinks", &note.backlinks);
        print_edges("broken", &note.broken);
    } else {
        println!("{} {}", note.id, note.path);
        println!("{}", note.title);
        println!("{}", note.body);
        print_edges("outgoing", &note.outgoing);
        print_edges("backlinks", &note.backlinks);
    }
    ExitCode::SUCCESS
}

fn print_edges(label: &str, edges: &[mnemic_core::LinkEdge]) {
    println!("{label}:");
    for edge in edges {
        let target = edge.target.as_deref().unwrap_or("-");
        println!("  {} {} -> {target}", edge.kind, edge.raw);
    }
}

fn print_value(value: Value) -> ExitCode {
    match serde_json::to_string_pretty(&value) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

fn init(json: bool, vault: Option<PathBuf>, actor: Option<String>) -> ExitCode {
    let target = vault.unwrap_or_else(|| PathBuf::from("."));
    let report = init_vault(InitRequest {
        target: &target,
        actor: actor.as_deref(),
        readme_template: include_str!("../templates/README.md"),
    });
    match report {
        Ok(report) => emit_init(json, &report),
        Err(err) => {
            let (code, message) = fatal_from_init(err);
            emit(json, fatal_envelope(code, message), exit_code(code))
        }
    }
}

fn fatal_from_init(err: InitError) -> (EnvelopeCode, String) {
    match err {
        InitError::Usage(message) => (EnvelopeCode::Usage, message),
        InitError::Io(message) => (EnvelopeCode::Io, message),
        InitError::LockHeld { .. } => (EnvelopeCode::LockHeld, err.to_string()),
        InitError::NestedVault(_) => (EnvelopeCode::NestedVault, err.to_string()),
        InitError::Unsupported { .. } => (EnvelopeCode::UnsupportedVaultVersion, err.to_string()),
        InitError::BadConfig(message) => (EnvelopeCode::BadConfig, message),
    }
}

fn emit_init(json: bool, report: &mnemic_core::InitReport) -> ExitCode {
    if json {
        let envelope = InitSuccess {
            ok: true,
            data: InitData {
                vault: report.vault.display().to_string(),
                created: report.created.clone(),
                skipped: report.skipped.clone(),
            },
            error: None,
        };
        match serde_json::to_string_pretty(&envelope) {
            Ok(text) => println!("{text}"),
            Err(err) => {
                eprintln!("{err}");
                return ExitCode::from(1);
            }
        }
    } else {
        for path in &report.created {
            println!("created {path}");
        }
        for path in &report.skipped {
            println!("skipped {path}");
        }
    }
    ExitCode::SUCCESS
}

fn resolve_root(
    flag: Option<&Path>,
    start: Option<&Path>,
) -> Result<PathBuf, (EnvelopeCode, String)> {
    let default = PathBuf::from(".");
    let start = start.unwrap_or(default.as_path());
    mnemic_core::open_config(start, flag)
        .map(|(root, _config)| root)
        .map_err(fatal_from_vault)
}

fn opened_vault(
    flag: Option<&Path>,
    start: Option<&Path>,
) -> Result<mnemic_core::Vault, (EnvelopeCode, String)> {
    let root = resolve_root(flag, start)?;
    mnemic_core::load_vault(&root).map_err(fatal_from_vault)
}

fn check_fix_cmd(path: Option<PathBuf>, json: bool, vault: Option<PathBuf>) -> ExitCode {
    let vault = match opened_vault(vault.as_deref(), path.as_deref()) {
        Ok(vault) => vault,
        Err((code, message)) => {
            return emit(json, fatal_envelope(code, message), exit_code(code));
        }
    };
    let target = path.unwrap_or_else(|| vault.root.clone());
    match check_fix(FixRequest {
        vault: &vault,
        path: &target,
        clock: &SystemClock,
        lock_timeout: mnemic_core::write::DEFAULT_LOCK_TIMEOUT,
    }) {
        Ok(outcome) => {
            let envelope = fix_envelope(&outcome);
            let status = if outcome.report.findings.is_empty() {
                0
            } else {
                exit_code(EnvelopeCode::CheckFailed)
            };
            if !json {
                if let Some(data) = &envelope.data {
                    if let Some(changes) = &data.changes {
                        for change in changes {
                            let field = match change.field {
                                mnemic_core::FixField::Id => "id",
                                mnemic_core::FixField::Type => "type",
                            };
                            println!("{field} {} {}", change.path, change.new);
                        }
                    }
                }
            }
            emit(json, envelope, status)
        }
        Err(err) => {
            let (code, message) = fatal_from_fix(err);
            emit(json, fatal_envelope(code, message), exit_code(code))
        }
    }
}

fn fatal_from_fix(err: FixError) -> (EnvelopeCode, String) {
    match err {
        FixError::Usage(message) => (EnvelopeCode::Usage, message),
        FixError::Io(message) => (EnvelopeCode::Io, message),
        FixError::LockHeld { .. } => (EnvelopeCode::LockHeld, err.to_string()),
        FixError::Outside(message) => (EnvelopeCode::OutsideVault, message),
    }
}

fn check(path: Option<PathBuf>, json: bool, vault: Option<PathBuf>, fix: bool) -> ExitCode {
    if fix {
        return check_fix_cmd(path, json, vault);
    }
    let vault = match opened_vault(vault.as_deref(), path.as_deref()) {
        Ok(vault) => vault,
        Err((code, message)) => {
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
        Ok(report) if report.findings.is_empty() => emit(json, success_envelope(), 0),
        Ok(report) => {
            let envelope = findings_envelope(report.findings, Vec::new(), report.details);
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

#[derive(Serialize)]
struct SkillEnvelope<'a> {
    ok: bool,
    data: Option<SkillData<'a>>,
    error: Option<Value>,
}

#[derive(Serialize)]
struct SkillData<'a> {
    name: &'a str,
    version: &'a str,
    content: &'a str,
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
        EnvelopeCode::NestedVault => "NESTED_VAULT",
        EnvelopeCode::NotFound => "NOT_FOUND",
        EnvelopeCode::Ambiguous => "AMBIGUOUS",
        EnvelopeCode::TimelineDisabled => "TIMELINE_DISABLED",
        EnvelopeCode::TimelineHeadingAmbiguous => "TIMELINE_HEADING_AMBIGUOUS",
        EnvelopeCode::OutsideVault => "OUTSIDE_VAULT",
        EnvelopeCode::UnknownType => "UNKNOWN_TYPE",
        EnvelopeCode::BadId => "BAD_ID",
        EnvelopeCode::IdExists => "ID_EXISTS",
        EnvelopeCode::SlugCollision => "SLUG_COLLISION",
        EnvelopeCode::MissingTemplate => "MISSING_TEMPLATE",
        EnvelopeCode::DestExists => "DEST_EXISTS",
        EnvelopeCode::NoteArchived => "NOTE_ARCHIVED",
        EnvelopeCode::AliasNotUnique => "ALIAS_NOT_UNIQUE",
        EnvelopeCode::RawImmutable => "RAW_IMMUTABLE",
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

    /// A success section is a `##` block whose heading names the subcommand, or
    /// whose first body line is `mnemic <name> ...`, and that states a success
    /// envelope (`ok: true` or the word success).
    fn success_section(markdown: &str, name: &str) -> bool {
        markdown.split("\n## ").skip(1).any(|section| {
            let mut lines = section.lines();
            let heading = lines.next().unwrap_or("");
            let first_body = lines.find(|line| !line.trim().is_empty()).unwrap_or("");
            let named =
                word(heading, name) || (word(first_body, name) && first_body.contains("mnemic"));
            named
                && (section.contains("ok: true")
                    || section.to_ascii_lowercase().contains("success"))
        })
    }

    fn word(hay: &str, name: &str) -> bool {
        hay.split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| token.eq_ignore_ascii_case(name))
    }

    #[test]
    fn every_subcommand_has_a_success_section() {
        let (_schema, markdown) = mnemic_core::render_contract().expect("render");
        let missing: Vec<_> = Cli::command()
            .get_subcommands()
            .map(|command| command.get_name().to_string())
            .filter(|name| !success_section(&markdown, name))
            .collect();
        assert!(
            missing.is_empty(),
            "contract success sections missing for: {}",
            missing.join(", ")
        );
    }
}

#[cfg(test)]
mod readme_contract {
    use super::Cli;
    use clap::CommandFactory;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn clap_verbs() -> BTreeSet<String> {
        Cli::command()
            .get_subcommands()
            .map(|command| command.get_name().to_string())
            .collect()
    }

    fn readme_verbs() -> BTreeSet<String> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../README.md");
        let readme = std::fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!("read {}: {err}", path.display());
        });
        let mut in_section = false;
        let mut names = BTreeSet::new();
        for line in readme.lines() {
            if line.starts_with("## Commands") {
                in_section = true;
                continue;
            }
            if in_section && line.starts_with("## ") {
                break;
            }
            if !in_section {
                continue;
            }
            let rest = line.trim().strip_prefix("- `").unwrap_or("");
            let name = rest.split('`').next().unwrap_or("");
            if !name.is_empty() && !name.contains(char::is_whitespace) {
                names.insert(name.to_string());
            }
        }
        names
    }

    #[test]
    fn readme_command_list_matches_clap() {
        let from_readme = readme_verbs();
        let from_clap = clap_verbs();
        assert_eq!(
            from_readme, from_clap,
            "README ## Commands bullets must be exactly the clap subcommands"
        );
        for absent in ["index", "migrate", "mcp"] {
            assert!(
                !from_clap.contains(absent),
                "{absent} must not be a shipped command"
            );
        }
    }
}
