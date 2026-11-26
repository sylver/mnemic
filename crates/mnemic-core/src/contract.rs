use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

/// Top-level `mnemic check --json` document.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct CheckEnvelope {
    pub ok: bool,
    pub data: Option<CheckData>,
    pub error: Option<ErrorObject>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct CheckData {
    pub errors: Vec<Finding>,
    pub warnings: Vec<Finding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Finding {
    pub code: FindingCode,
    pub message: String,
    pub path: String,
    /// Wikilink or typed-link text when `code` is `AMBIGUOUS`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// Sorted candidate paths when `code` is `AMBIGUOUS`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matches: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ErrorObject {
    pub code: EnvelopeCode,
    pub message: String,
    pub details: BTreeMap<String, Value>,
}

/// Per-note and later vault-wide check codes. These appear in `data.errors`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
pub enum FindingCode {
    #[serde(rename = "BAD_FRONTMATTER")]
    BadFrontmatter,
    #[serde(rename = "BAD_ID")]
    BadId,
    #[serde(rename = "MISSING_FIELD")]
    MissingField,
    #[serde(rename = "UNKNOWN_TYPE")]
    UnknownType,
    #[serde(rename = "ALIAS_CONTACT")]
    AliasContact,
    #[serde(rename = "SCHEMA_VERSION_AHEAD")]
    SchemaVersionAhead,
    #[serde(rename = "INVALID_PACK")]
    InvalidPack,
    #[serde(rename = "DUPLICATE_ID")]
    DuplicateId,
    #[serde(rename = "ALIAS_NOT_UNIQUE")]
    AliasNotUnique,
    #[serde(rename = "SUPERSEDED_WITHOUT_INCOMING")]
    SupersededWithoutIncoming,
    #[serde(rename = "SUPERSEDES_CYCLE")]
    SupersedesCycle,
    #[serde(rename = "BROKEN_LINK")]
    BrokenLink,
    #[serde(rename = "SHA256_DRIFT")]
    Sha256Drift,
    #[serde(rename = "AMBIGUOUS")]
    Ambiguous,
}

/// Codes on `error.code`. Findings use `CHECK_FAILED`. The rest are fatal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub enum EnvelopeCode {
    #[serde(rename = "BAD_CONFIG")]
    BadConfig,
    #[serde(rename = "CHECK_FAILED")]
    CheckFailed,
    #[serde(rename = "IO")]
    Io,
    #[serde(rename = "LOCK_HELD")]
    LockHeld,
    #[serde(rename = "UNSUPPORTED_VAULT_VERSION")]
    UnsupportedVaultVersion,
    #[serde(rename = "USAGE")]
    Usage,
    #[serde(rename = "VAULT_NOT_FOUND")]
    VaultNotFound,
    #[serde(rename = "NESTED_VAULT")]
    NestedVault,
    #[serde(rename = "NOT_FOUND")]
    NotFound,
    #[serde(rename = "AMBIGUOUS")]
    Ambiguous,
    #[serde(rename = "TIMELINE_DISABLED")]
    TimelineDisabled,
    #[serde(rename = "TIMELINE_HEADING_AMBIGUOUS")]
    TimelineHeadingAmbiguous,
    #[serde(rename = "OUTSIDE_VAULT")]
    OutsideVault,
}

pub fn exit_code(code: EnvelopeCode) -> i32 {
    match code {
        EnvelopeCode::Io => 1,
        EnvelopeCode::LockHeld => 75,
        EnvelopeCode::BadConfig
        | EnvelopeCode::CheckFailed
        | EnvelopeCode::UnsupportedVaultVersion
        | EnvelopeCode::Usage
        | EnvelopeCode::VaultNotFound
        | EnvelopeCode::NestedVault
        | EnvelopeCode::NotFound
        | EnvelopeCode::Ambiguous
        | EnvelopeCode::TimelineDisabled
        | EnvelopeCode::TimelineHeadingAmbiguous
        | EnvelopeCode::OutsideVault => 2,
    }
}

pub fn success_envelope() -> CheckEnvelope {
    CheckEnvelope {
        ok: true,
        data: Some(CheckData {
            errors: Vec::new(),
            warnings: Vec::new(),
        }),
        error: None,
    }
}

pub fn findings_envelope(
    mut errors: Vec<Finding>,
    warnings: Vec<Finding>,
    details: BTreeMap<String, Value>,
) -> CheckEnvelope {
    errors.sort_by(|a, b| (&a.path, &a.code, &a.message).cmp(&(&b.path, &b.code, &b.message)));
    let count = errors.len();
    let label = if count == 1 { "error" } else { "errors" };
    CheckEnvelope {
        ok: false,
        data: Some(CheckData { errors, warnings }),
        error: Some(ErrorObject {
            code: EnvelopeCode::CheckFailed,
            message: format!("check found {count} {label}"),
            details,
        }),
    }
}

pub fn fatal_envelope(code: EnvelopeCode, message: impl Into<String>) -> CheckEnvelope {
    CheckEnvelope {
        ok: false,
        data: None,
        error: Some(ErrorObject {
            code,
            message: message.into(),
            details: BTreeMap::new(),
        }),
    }
}

pub fn to_json(envelope: &CheckEnvelope) -> Result<String, serde_json::Error> {
    let mut text = serde_json::to_string_pretty(envelope)?;
    text.push('\n');
    Ok(text)
}

pub fn render_contract() -> Result<(String, String), serde_json::Error> {
    let schema = schemars::schema_for!(CheckEnvelope);
    let mut schema_json = serde_json::to_string_pretty(&schema)?;
    schema_json.push('\n');
    Ok((schema_json, render_markdown()))
}

fn render_markdown() -> String {
    let mut out = String::from(
        "# Check contract\n\n\
         Generated from the `CheckEnvelope` types in `mnemic-core`. \
         Do not edit this file by hand. Run `cargo run -p mnemic-core --bin mnemic-contract`.\n\n\
         ## Envelope\n\n\
         `mnemic check --json` prints one JSON document.\n\n\
         A clean tree is `ok: true`, `data.errors` and `data.warnings` empty, `error: null`, exit 0.\n\n\
         Findings (`BAD_ID`, `MISSING_FIELD`, `BAD_FRONTMATTER`, `UNKNOWN_TYPE`, \
         `ALIAS_CONTACT`, `SCHEMA_VERSION_AHEAD`, `INVALID_PACK`, `DUPLICATE_ID`, \
         `ALIAS_NOT_UNIQUE`, `SUPERSEDED_WITHOUT_INCOMING`, `SUPERSEDES_CYCLE`, \
         `BROKEN_LINK`, `SHA256_DRIFT`, `AMBIGUOUS`) use \
         `ok: false`, `data` with those findings, and `error.code` `CHECK_FAILED`. Exit 2.\n\n\
         Fatal codes use `ok: false`, `data: null`, and `error.code` set to the fatal code. \
         `VAULT_NOT_FOUND`, `USAGE`, `BAD_CONFIG`, `UNSUPPORTED_VAULT_VERSION`, and \
         `NESTED_VAULT` are fatal. They are never entries in `data.errors`. \
         `init` returns `NESTED_VAULT` when a parent of the target already has `mnemic.toml`.\n\n\
         ## Exit codes\n\n\
         | Code | Exit |\n\
         | --- | --- |\n",
    );
    let codes = [
        EnvelopeCode::Usage,
        EnvelopeCode::VaultNotFound,
        EnvelopeCode::BadConfig,
        EnvelopeCode::UnsupportedVaultVersion,
        EnvelopeCode::Io,
        EnvelopeCode::LockHeld,
        EnvelopeCode::CheckFailed,
        EnvelopeCode::NestedVault,
        EnvelopeCode::NotFound,
        EnvelopeCode::Ambiguous,
        EnvelopeCode::TimelineDisabled,
        EnvelopeCode::TimelineHeadingAmbiguous,
        EnvelopeCode::OutsideVault,
    ];
    for code in codes {
        let name = serde_json::to_string(&code).unwrap_or_else(|_| "\"?\"".to_string());
        let name = name.trim_matches('"');
        out.push_str(&format!("| `{name}` | {} |\n", exit_code(code)));
    }
    out.push_str(
        "\n`LOCK_HELD` is reserved at exit 75. `check` does not take the vault lock.\n\n\
         Unexpected I/O uses `IO` and exit 1. Every other fatal code and `CHECK_FAILED` use exit 2.\n\n\
         ## Finding codes\n\n\
         | Code | Meaning |\n\
         | --- | --- |\n\
         | `BAD_ID` | `id` is present and is not a 26-character ULID |\n\
         | `MISSING_FIELD` | A required core field is absent, a source note has neither `path` nor `url`, or a required type field such as journal `date` is absent |\n\
         | `BAD_FRONTMATTER` | No frontmatter, a non-mapping, a duplicate YAML key, an unterminated `---` fence, or a present core field with an invalid value |\n\
         | `UNKNOWN_TYPE` | `type` is not in the loaded pack |\n\
         | `ALIAS_CONTACT` | An `aliases` entry is an email address or a phone number |\n\
         | `SCHEMA_VERSION_AHEAD` | The note `schema_version` is newer than the loaded type |\n\
         | `INVALID_PACK` | A type pack file is missing a required key, is not valid TOML, or is a symlink or path outside the vault |\n\
         | `DUPLICATE_ID` | Two notes use one `id` |\n\
         | `ALIAS_NOT_UNIQUE` | One alias, after NFC case-folding, maps to two ids |\n\
         | `SUPERSEDED_WITHOUT_INCOMING` | `status` is `superseded` and no other note points at it with `supersedes` |\n\
         | `SUPERSEDES_CYCLE` | `supersedes` links form a cycle |\n\
         | `BROKEN_LINK` | A wikilink, embed, typed link, or `supersedes` target does not resolve |\n\
         | `SHA256_DRIFT` | A source note `sha256` does not match the raw file at `path`, or `path` is not a regular file inside the vault |\n\
         | `AMBIGUOUS` | A link matches more than one note. `error.details.candidates` lists `path`, `link`, and `matches` |\n\n\
         The JSON Schema for the envelope is [`check.schema.json`](check.schema.json).\n\n\
         ## `mnemic log`\n\n\
         `mnemic log <note> <text> --json` prints one JSON document. `<note>` is a path, id, slug, or alias, resolved inside the write lock.\n\n\
         Success is `ok: true`, `error: null`, exit 0, and `data` with `id`, `path`, `noop`, `line`, and `heading_created`. `line` is the timeline item, including the `- ` marker. `noop` is true when that exact line is already on the page. `heading_created` is true when this call added `## Timeline`.\n\n\
         `NOT_FOUND`, `AMBIGUOUS`, `USAGE`, `TIMELINE_DISABLED`, `TIMELINE_HEADING_AMBIGUOUS`, and `OUTSIDE_VAULT` are fatal for `log`. Exit 2. `LOCK_HELD` is exit 75. `IO` is exit 1. `OUTSIDE_VAULT` means the resolved path canonicalizes outside the vault, including an outside symlink. The check finding `AMBIGUOUS` is a different code from the fatal `AMBIGUOUS` on `log`.\n\n\
         ## Search\n\n\
         `mnemic search <query> [--type <type>] [--tag <tag>] [--since <date>] [--limit <n>] [--json]` \
         scans the parsed note-file set. There is no index command and no query language. \
         Tokens are whitespace-separated. Matching is case-insensitive Unicode lowercase with no \
         diacritic folding. Every token must occur in the title, an alias, or the body. \
         `-`, `\"`, and `:` are literal characters.\n\n\
         `--type` is an exact type name. `--tag` matches a `tags` entry that equals the value \
         or starts with `<value>/`. `--since` keeps notes whose `updated` is that ISO date or \
         datetime, inclusive. A date is midnight UTC. The default `--limit` is 20. \
         `--limit` of zero or less and an empty query are `USAGE`, exit 2. \
         Zero hits is `ok: true`, `hits: []`, exit 0.\n\n\
         Ranking puts a title match ahead of a body or alias match, then a higher token count, \
         then a later `updated`, then `id` ascending. `score` is that order starting at 1. \
         `section` is `truth` or `timeline`. Timeline is the body from a line `## Timeline` onward. \
         `snippet` is the first matching title, alias, or line, cut at 200 characters.\n\n\
         An entry that is not a regular file inside the vault is not a note, so it is not a hit and search does not error. \
         `search` also returns `VAULT_NOT_FOUND` and `IO`. It does not return an FTS syntax error.\n\n\
         `--json` success:\n\n\
         `ok`, `data.query`, `data.hits[]` with `id`, `path`, `title`, `type`, `snippet`, \
         `section`, `score`, and `error: null`.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_held_exits_75() {
        assert_eq!(exit_code(EnvelopeCode::LockHeld), 75);
    }

    #[test]
    fn io_exits_1_and_other_fatals_exit_2() {
        assert_eq!(exit_code(EnvelopeCode::Io), 1);
        assert_eq!(exit_code(EnvelopeCode::BadConfig), 2);
        assert_eq!(exit_code(EnvelopeCode::VaultNotFound), 2);
        assert_eq!(exit_code(EnvelopeCode::UnsupportedVaultVersion), 2);
        assert_eq!(exit_code(EnvelopeCode::Usage), 2);
        assert_eq!(exit_code(EnvelopeCode::CheckFailed), 2);
        assert_eq!(exit_code(EnvelopeCode::NestedVault), 2);
        assert_eq!(exit_code(EnvelopeCode::NotFound), 2);
        assert_eq!(exit_code(EnvelopeCode::Ambiguous), 2);
        assert_eq!(exit_code(EnvelopeCode::TimelineDisabled), 2);
        assert_eq!(exit_code(EnvelopeCode::TimelineHeadingAmbiguous), 2);
        assert_eq!(exit_code(EnvelopeCode::OutsideVault), 2);
    }
}
