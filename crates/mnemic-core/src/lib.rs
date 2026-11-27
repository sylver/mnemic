use std::time::SystemTime;

use thiserror::Error;

mod check;
mod clock;
mod contract;
mod edit;
mod fields;
mod fix;
mod frontmatter;
mod index;
mod init;
mod log;
mod mv;
mod new;
mod pack;
mod read;
mod search;
mod set;
mod ulid;
mod vault;
pub mod write;

pub use check::{check_path, invalid_pack_finding, is_note, CheckFatal, CheckReport};
pub use clock::{Clock, FixedClock, SystemClock};
pub use contract::{
    exit_code, fatal_envelope, fatal_envelope_with, findings_envelope, render_contract,
    render_skill_contract, render_skill_types, success_envelope, to_json, CheckData, CheckEnvelope,
    EnvelopeCode, ErrorObject, Finding, FindingCode, FixChange, FixField,
};
pub use edit::{EditError, FrontmatterEdit};
pub use fix::{check_fix, fix_envelope, FixError, FixOutcome, FixRequest, FIX_ID_SEED};
pub use frontmatter::{parse_note, ByteSpan, FrontmatterError, KeySpan, ParsedNote};
pub use index::{index_vault, IndexedNote, LinkEdge, LinkSets, ResolveError, Resolved, VaultIndex};
pub use init::{init_vault, InitData, InitError, InitReport, InitRequest, InitSuccess};
pub use log::{log_note, LogData, LogError, LogRequest, LogResult, LogSuccess};
pub use mv::{move_note, MoveData, MoveError, MoveRequest, MoveResult, MoveSuccess};
pub use new::{create_note, slugify, NewData, NewError, NewRequest, NewResult, NewSuccess};
pub use pack::{
    core_pack, core_template, load_overlay_dir, load_pack, resolve_pack, template_body, FieldDef,
    FieldKind, Pack, PackError, TypeDef,
};
pub use read::{read_note, Candidate, NoteView, ReadError};
pub use search::{
    search_vault, SearchData, SearchEnvelope, SearchError, SearchHit, SearchRequest, SearchSection,
};
pub use set::{set_fields, SetData, SetError, SetRequest, SetResult, SetSuccess};
pub use ulid::{is_ulid, UlidGenerator};
pub use vault::{
    load_vault, open_config, open_vault, parse_vault_config, Vault, VaultConfig, VaultError,
    SUPPORTED_VAULT_SCHEMA_VERSION,
};

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0} is empty")]
    Empty(&'static str),
    /// Another process holds the vault write lock after the wait elapsed.
    #[error("lock held by pid {pid} since {}", since_secs(.since))]
    LockHeld { pid: u32, since: SystemTime },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn since_secs(since: &SystemTime) -> u64 {
    since
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty(left), Self::Empty(right)) => left == right,
            (
                Self::LockHeld {
                    pid: left_pid,
                    since: left_since,
                },
                Self::LockHeld {
                    pid: right_pid,
                    since: right_since,
                },
            ) => left_pid == right_pid && left_since == right_since,
            (Self::Io(left), Self::Io(right)) => {
                left.kind() == right.kind() && left.to_string() == right.to_string()
            }
            _ => false,
        }
    }
}

impl Eq for Error {}

pub fn require_non_empty<'a>(label: &'static str, value: &'a str) -> Result<&'a str, Error> {
    if value.is_empty() {
        Err(Error::Empty(label))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_value() {
        assert_eq!(require_non_empty("name", ""), Err(Error::Empty("name")));
    }

    #[test]
    fn keeps_non_empty_value() {
        assert_eq!(require_non_empty("name", "mnemic"), Ok("mnemic"));
    }
}
