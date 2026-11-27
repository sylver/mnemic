mod user_config;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use std::collections::BTreeMap;

use clap::Parser;
use mnemic::cli::{Cli, Command};
use mnemic_core::{
    check_fix, create_note, exit_code, fatal_envelope, fatal_envelope_with, findings_envelope,
    fix_envelope, init_vault, log_note, move_note, read_note, search_vault, set_fields,
    success_envelope, to_json, CheckEnvelope, CheckFatal, EnvelopeCode, FixError, FixRequest,
    InitData, InitError, InitRequest, InitSuccess, LogError, LogRequest, MoveError, MoveRequest,
    NewError, NewRequest, NoteView, ReadError, SearchData, SearchEnvelope, SearchError,
    SearchRequest, SetError, SetRequest, SystemClock, UlidGenerator, VaultError,
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
            field,
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
            field,
            json,
            vault,
        }),
        Command::Set {
            note,
            field,
            add,
            json,
            vault,
        } => set_note(note, field, add, json, vault),
        Command::Mv {
            note,
            dest,
            no_rewrite,
            json,
            vault,
        } => mv(note, dest, no_rewrite, json, vault),
        Command::Use { path, clear, json } => use_vault(path, clear, json),
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
    let root = match open_command(vault.as_deref(), true, json) {
        Ok(vault) => vault.root,
        Err(code) => return code,
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
    field: Vec<String>,
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
        field,
        json,
        vault,
    } = args;
    let root = match resolve_command(vault.as_deref(), true, json) {
        Ok(root) => root,
        Err(code) => return code,
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
        fields: &field,
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

fn set_note(
    note: String,
    field: Vec<String>,
    add: Vec<String>,
    json: bool,
    vault: Option<PathBuf>,
) -> ExitCode {
    let root = match resolve_command(vault.as_deref(), true, json) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let result = set_fields(SetRequest {
        vault: &root,
        note: &note,
        fields: &field,
        adds: &add,
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
                println!("noop {}", result.path);
            } else {
                println!("{} {}", result.path, result.changed.join(" "));
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            let (code, message, details) = fatal_from_set(err);
            let envelope = if details.is_empty() {
                fatal_envelope(code, message)
            } else {
                fatal_envelope_with(code, message, details)
            };
            emit(json, envelope, exit_code(code))
        }
    }
}

fn fatal_from_set(err: SetError) -> (EnvelopeCode, String, BTreeMap<String, Value>) {
    match err {
        SetError::Usage(message) => (EnvelopeCode::Usage, message, BTreeMap::new()),
        SetError::NotFound(message) => (EnvelopeCode::NotFound, message, BTreeMap::new()),
        SetError::Ambiguous { candidates } => {
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
        SetError::OutsideVault(message) => (EnvelopeCode::OutsideVault, message, BTreeMap::new()),
        SetError::UnknownType(message) => (EnvelopeCode::UnknownType, message, BTreeMap::new()),
        SetError::LockHeld { .. } => (EnvelopeCode::LockHeld, err.to_string(), BTreeMap::new()),
        SetError::Io(message) => (EnvelopeCode::Io, message, BTreeMap::new()),
        SetError::Vault(message) => {
            let (code, message) = fatal_from_vault_message(&message);
            (code, message, BTreeMap::new())
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
    let root = match open_command(vault.as_deref(), true, json) {
        Ok(vault) => vault.root,
        Err(code) => return code,
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
    let root = match open_command(vault.as_deref(), true, json) {
        Ok(vault) => vault.root,
        Err(code) => return code,
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
    let root = match open_command(vault.as_deref(), true, json) {
        Ok(vault) => vault.root,
        Err(code) => return code,
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

fn check_fix_cmd(path: Option<PathBuf>, json: bool, vault: Option<PathBuf>) -> ExitCode {
    let start = path.clone().unwrap_or_else(|| PathBuf::from("."));
    let vault = match open_at(&start, vault.as_deref(), path.is_none(), json) {
        Ok(vault) => vault,
        Err(code) => return code,
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
    let start = path.clone().unwrap_or_else(|| PathBuf::from("."));
    let vault = match open_at(&start, vault.as_deref(), path.is_none(), json) {
        Ok(vault) => vault,
        Err(code) => return code,
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

enum Source {
    Cwd,
    Default,
    Flag,
}

struct OpenError {
    code: EnvelopeCode,
    message: String,
    details: BTreeMap<String, Value>,
}

fn open_command(
    explicit: Option<&Path>,
    allow_default: bool,
    json: bool,
) -> Result<mnemic_core::Vault, ExitCode> {
    let root = resolve_command(explicit, allow_default, json)?;
    match mnemic_core::load_vault(&root) {
        Ok(vault) => Ok(vault),
        Err(err) => Err(emit_open(json, open_from_vault(err))),
    }
}

fn resolve_command(
    explicit: Option<&Path>,
    allow_default: bool,
    json: bool,
) -> Result<PathBuf, ExitCode> {
    match resolve_root(Path::new("."), explicit, allow_default) {
        Ok((root, _)) => Ok(root),
        Err(err) => Err(emit_open(json, err)),
    }
}

fn open_at(
    start: &Path,
    explicit: Option<&Path>,
    allow_default: bool,
    json: bool,
) -> Result<mnemic_core::Vault, ExitCode> {
    let root = match resolve_root(start, explicit, allow_default) {
        Ok((root, _)) => root,
        Err(err) => return Err(emit_open(json, err)),
    };
    match mnemic_core::load_vault(&root) {
        Ok(vault) => Ok(vault),
        Err(err) => Err(emit_open(json, open_from_vault(err))),
    }
}

/// Tier 1 is `--vault` (exact). Tier 2 walks from `start`. Tier 3 reads the
/// user config only when the walk misses and `allow_default` is set.
fn resolve_root(
    start: &Path,
    explicit: Option<&Path>,
    allow_default: bool,
) -> Result<(PathBuf, Source), OpenError> {
    if let Some(path) = explicit {
        return match mnemic_core::open_config(start, Some(path)) {
            Ok((root, _)) => Ok((root, Source::Flag)),
            Err(err) => Err(open_from_vault(err)),
        };
    }
    let start = searched_from(start);
    match mnemic_core::open_config(&start, None) {
        Ok((root, _)) => Ok((root, Source::Cwd)),
        Err(VaultError::NotFound) if allow_default => open_default(&start),
        Err(err) => Err(open_from_vault(err)),
    }
}

fn open_default(start: &Path) -> Result<(PathBuf, Source), OpenError> {
    let searched = searched_from(start);
    let Some(config) = config_file() else {
        return Err(no_default(&searched));
    };
    let bound = match user_config::read(&config) {
        Ok(bound) => bound,
        Err(message) => {
            return Err(OpenError {
                code: EnvelopeCode::BadConfig,
                message,
                details: BTreeMap::new(),
            });
        }
    };
    let Some(bound) = bound else {
        return Err(no_default(&searched));
    };
    match mnemic_core::open_config(&bound, Some(&bound)) {
        Ok((root, _)) => Ok((root, Source::Default)),
        Err(VaultError::NotFound) => Err(stale_default(&searched, &bound, &config)),
        Err(VaultError::Io(message))
            if message.contains("not a directory") || message.contains("does not exist") =>
        {
            Err(stale_default(&searched, &bound, &config))
        }
        Err(err) => Err(open_from_vault(err)),
    }
}

fn config_file() -> Option<PathBuf> {
    user_config::config_path(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

fn searched_from(start: &Path) -> PathBuf {
    let joined = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(start)
    };
    std::fs::canonicalize(&joined).unwrap_or(joined)
}

fn canonical_root(root: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(root).map_err(|err| err.to_string())
}

fn no_default(searched: &Path) -> OpenError {
    let mut details = BTreeMap::new();
    details.insert(
        "searched_from".to_string(),
        Value::String(searched.display().to_string()),
    );
    OpenError {
        code: EnvelopeCode::VaultNotFound,
        message: format!(
            "no mnemic.toml in {} or its parents, and no default vault. Run 'mnemic init' to create one or 'mnemic use <path>' to set a default.",
            searched.display()
        ),
        details,
    }
}

fn stale_default(searched: &Path, default: &Path, config: &Path) -> OpenError {
    let mut details = BTreeMap::new();
    details.insert(
        "config".to_string(),
        Value::String(config.display().to_string()),
    );
    details.insert(
        "default".to_string(),
        Value::String(default.display().to_string()),
    );
    details.insert(
        "searched_from".to_string(),
        Value::String(searched.display().to_string()),
    );
    OpenError {
        code: EnvelopeCode::VaultNotFound,
        message: format!(
            "default vault {} has no mnemic.toml (set in {}). Run 'mnemic use <path>' or 'mnemic use --clear'.",
            default.display(),
            config.display()
        ),
        details,
    }
}

fn open_from_vault(err: VaultError) -> OpenError {
    let (code, message) = fatal_from_vault(err);
    OpenError {
        code,
        message,
        details: BTreeMap::new(),
    }
}

fn emit_open(json: bool, err: OpenError) -> ExitCode {
    let envelope = if err.details.is_empty() {
        fatal_envelope(err.code, err.message)
    } else {
        fatal_envelope_with(err.code, err.message, err.details)
    };
    emit(json, envelope, exit_code(err.code))
}

fn use_vault(path: Option<PathBuf>, clear: bool, json: bool) -> ExitCode {
    if clear {
        return use_clear(json);
    }
    if let Some(path) = path {
        return use_set(&path, json);
    }
    use_show(json)
}

fn use_show(json: bool) -> ExitCode {
    match diagnose() {
        Ok(report) => emit_use(json, &report, true),
        Err(err) => emit_open(json, err),
    }
}

fn use_clear(json: bool) -> ExitCode {
    let Some(config) = config_file() else {
        return emit_open(
            json,
            OpenError {
                code: EnvelopeCode::Usage,
                message: "set HOME or XDG_CONFIG_HOME".to_string(),
                details: BTreeMap::new(),
            },
        );
    };
    let changed = match user_config::write(&config, None) {
        Ok(changed) => changed,
        Err(message) => {
            return emit_open(
                json,
                OpenError {
                    code: EnvelopeCode::Io,
                    message,
                    details: BTreeMap::new(),
                },
            );
        }
    };
    match diagnose() {
        Ok(mut report) => {
            report.changed = changed;
            report.default = None;
            emit_use(json, &report, false)
        }
        Err(err) => emit_open(json, err),
    }
}

fn use_set(path: &Path, json: bool) -> ExitCode {
    let Some(config) = config_file() else {
        return emit_open(
            json,
            OpenError {
                code: EnvelopeCode::Usage,
                message: "set HOME or XDG_CONFIG_HOME".to_string(),
                details: BTreeMap::new(),
            },
        );
    };
    if path.to_str().is_none() {
        return emit_open(
            json,
            OpenError {
                code: EnvelopeCode::Usage,
                message: "vault path must be UTF-8".to_string(),
                details: BTreeMap::new(),
            },
        );
    }
    let opened = match mnemic_core::open_config(&searched_from(path), None) {
        Ok((root, _)) => root,
        Err(VaultError::NotFound) => {
            return emit_open(
                json,
                OpenError {
                    code: EnvelopeCode::VaultNotFound,
                    message: format!("no mnemic.toml in {} or its parents", path.display()),
                    details: BTreeMap::new(),
                },
            );
        }
        Err(err) => return emit_open(json, open_from_vault(err)),
    };
    let root = match canonical_root(&opened) {
        Ok(root) => root,
        Err(message) => {
            return emit_open(
                json,
                OpenError {
                    code: EnvelopeCode::Io,
                    message,
                    details: BTreeMap::new(),
                },
            );
        }
    };
    if root.to_str().is_none() {
        return emit_open(
            json,
            OpenError {
                code: EnvelopeCode::Usage,
                message: "vault path must be UTF-8".to_string(),
                details: BTreeMap::new(),
            },
        );
    }
    let changed = match user_config::write(&config, Some(&root)) {
        Ok(changed) => changed,
        Err(message) => {
            return emit_open(
                json,
                OpenError {
                    code: EnvelopeCode::Io,
                    message,
                    details: BTreeMap::new(),
                },
            );
        }
    };
    match diagnose() {
        Ok(mut report) => {
            report.changed = changed;
            emit_use(json, &report, false)
        }
        Err(err) => emit_open(json, err),
    }
}

struct UseReport {
    vault: Option<String>,
    source: Option<&'static str>,
    default: Option<String>,
    config: Option<String>,
    changed: bool,
}

fn diagnose() -> Result<UseReport, OpenError> {
    let config = config_file();
    let (default, config_text) = match &config {
        None => (None, None),
        Some(path) => {
            let bound = match user_config::read(path) {
                Ok(bound) => bound,
                Err(message) => {
                    return Err(OpenError {
                        code: EnvelopeCode::BadConfig,
                        message,
                        details: BTreeMap::new(),
                    });
                }
            };
            (
                bound.map(|path| path.display().to_string()),
                Some(path.display().to_string()),
            )
        }
    };
    let start = searched_from(Path::new("."));
    let (vault, source) = match mnemic_core::open_config(&start, None) {
        Ok((root, _)) => (Some(path_text(&root)?), Some("cwd")),
        Err(VaultError::NotFound) => match &default {
            Some(bound) => {
                match mnemic_core::open_config(Path::new(bound), Some(Path::new(bound))) {
                    Ok((root, _)) => (Some(path_text(&root)?), Some("default")),
                    Err(_) => (None, None),
                }
            }
            None => (None, None),
        },
        Err(err) => return Err(open_from_vault(err)),
    };
    Ok(UseReport {
        vault,
        source,
        default,
        config: config_text,
        changed: false,
    })
}

fn path_text(path: &Path) -> Result<String, OpenError> {
    let root = canonical_root(path).map_err(|message| OpenError {
        code: EnvelopeCode::Io,
        message,
        details: BTreeMap::new(),
    })?;
    root.to_str()
        .map(ToString::to_string)
        .ok_or_else(|| OpenError {
            code: EnvelopeCode::Usage,
            message: "vault path must be UTF-8".to_string(),
            details: BTreeMap::new(),
        })
}

fn emit_use(json: bool, report: &UseReport, show: bool) -> ExitCode {
    if json {
        let body = serde_json::json!({
            "ok": true,
            "data": {
                "vault": report.vault,
                "source": report.source,
                "default": report.default,
                "config": report.config,
                "changed": report.changed,
            },
            "error": null,
        });
        match serde_json::to_string_pretty(&body) {
            Ok(text) => println!("{text}"),
            Err(err) => {
                eprintln!("{err}");
                return ExitCode::from(1);
            }
        }
    } else if show {
        print_use_human(report);
    } else {
        println!("default {}", report.default.as_deref().unwrap_or("none"));
    }
    ExitCode::SUCCESS
}

fn print_use_human(report: &UseReport) {
    let vault = match (&report.vault, report.source) {
        (Some(path), Some(source)) => format!("{path} ({source})"),
        (Some(path), None) => path.clone(),
        _ => "none".to_string(),
    };
    println!("vault    {vault}");
    println!("default  {}", report.default.as_deref().unwrap_or("none"));
    println!("config   {}", report.config.as_deref().unwrap_or("none"));
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
