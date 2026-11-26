//! Vault paths are read and written only through the confined walk and the write primitive.
//!
//! A new `std::fs::read`, `read_to_string`, `read_dir`, `File::open`, `OpenOptions::open`,
//! `fs::canonicalize`, or `fs::metadata` in `mnemic-core` or `mnemic` library source fails
//! this test. The fix is `collect_notes`, `regular_file_inside`, `canonicalize_within`,
//! `VaultIndex::resolve_argument` (`confine_user_path`), `with_write_lock`, or
//! `WriteTxn::atomic_write`. Each name is exempt only in its defining file.
//! Do not reimplement, cache, or wrap `canonicalize_within`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// `(defining file, function)`. A same-named function in any other file is checked.
const SANCTIONED: &[(&str, &str)] = &[
    ("crates/mnemic-core/src/write/mod.rs", "atomic_write"),
    ("crates/mnemic-core/src/vault.rs", "canonicalize_within"),
    ("crates/mnemic-core/src/check.rs", "collect_notes"),
    ("crates/mnemic-core/src/index.rs", "confine_user_path"),
    ("crates/mnemic-core/src/check.rs", "regular_file_inside"),
    ("crates/mnemic-core/src/index.rs", "resolve_argument"),
    ("crates/mnemic-core/src/write/mod.rs", "with_write_lock"),
];

/// `(file, enclosing function, call, times)`.
///
/// `times` is 1 except where that function already makes the same call more
/// than once. A new `(file, function, call)` fails. A listed triple with no
/// remaining call fails. Shifting lines does not.
const CLOSED: &[(&str, &str, &str, u32)] = &[
    // Note bytes after regular_file_inside.
    ("crates/mnemic-core/src/check.rs", "check_path", "read", 1),
    ("crates/mnemic-core/src/fix.rs", "fix_locked", "read", 1),
    // Vault root and mnemic.toml. Not a note walk.
    ("crates/mnemic-core/src/index.rs", "load", "canonicalize", 1),
    ("crates/mnemic-core/src/index.rs", "read_hash", "read", 1),
    ("crates/mnemic-core/src/index.rs", "read_note", "read", 1),
    // init scaffolds a directory that is not a vault yet.
    // prepare_target and plan_gitignore already call these twice.
    (
        "crates/mnemic-core/src/init.rs",
        "classify_config",
        "read_to_string",
        1,
    ),
    (
        "crates/mnemic-core/src/init.rs",
        "directory_has_entries",
        "read_dir",
        1,
    ),
    (
        "crates/mnemic-core/src/init.rs",
        "enclosing_vault_for_missing",
        "canonicalize",
        1,
    ),
    (
        "crates/mnemic-core/src/init.rs",
        "plan_gitignore",
        "read",
        2,
    ),
    (
        "crates/mnemic-core/src/init.rs",
        "prepare_target",
        "canonicalize",
        2,
    ),
    ("crates/mnemic-core/src/log.rs", "log_locked", "read", 1),
    // Note bytes after check::regular_file_inside.
    ("crates/mnemic-core/src/mv.rs", "read_note_bytes", "read", 1),
    (
        "crates/mnemic-core/src/pack.rs",
        "load_overlay_dir",
        "read_dir",
        1,
    ),
    (
        "crates/mnemic-core/src/pack.rs",
        "load_overlay_dir",
        "read_to_string",
        1,
    ),
    (
        "crates/mnemic-core/src/pack.rs",
        "template_body",
        "read_to_string",
        1,
    ),
    (
        "crates/mnemic-core/src/read.rs",
        "read_confined",
        "read_to_string",
        1,
    ),
    (
        "crates/mnemic-core/src/read.rs",
        "read_note",
        "canonicalize",
        1,
    ),
    // Vault root and mnemic.toml. Not a note walk. Shared by open_config and load_vault.
    (
        "crates/mnemic-core/src/vault.rs",
        "read_config_text",
        "read_to_string",
        1,
    ),
    // Write-primitive helpers called by atomic_write / with_write_lock.
    // O_EXCL create of a new note.
    (
        "crates/mnemic-core/src/write/atomic.rs",
        "create_exclusive",
        "OpenOptions::open",
        1,
    ),
    // fsync the parent directory after the exclusive create.
    (
        "crates/mnemic-core/src/write/atomic.rs",
        "create_exclusive",
        "File::open",
        1,
    ),
    (
        "crates/mnemic-core/src/write/atomic.rs",
        "commit_temp",
        "File::open",
        1,
    ),
    // Directory fsync after the exclusive rename, same as commit_temp.
    (
        "crates/mnemic-core/src/write/atomic.rs",
        "rename_exclusive",
        "File::open",
        1,
    ),
    (
        "crates/mnemic-core/src/write/atomic.rs",
        "random_u64",
        "File::open",
        1,
    ),
    (
        "crates/mnemic-core/src/write/atomic.rs",
        "remove_leftover_temps",
        "read_dir",
        1,
    ),
    (
        "crates/mnemic-core/src/write/atomic.rs",
        "stage_temp",
        "OpenOptions::open",
        1,
    ),
    (
        "crates/mnemic-core/src/write/lock.rs",
        "acquire",
        "OpenOptions::open",
        1,
    ),
    (
        "crates/mnemic-core/src/write/lock.rs",
        "lock_path",
        "canonicalize",
        1,
    ),
    (
        "crates/mnemic-core/src/write/lock.rs",
        "read_holder",
        "read_to_string",
        1,
    ),
];

#[test]
fn confined_vault_io_stays_behind_the_walk() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found = Vec::new();
    for relative in ["crates/mnemic-core/src", "crates/mnemic/src"] {
        let dir = root.join(relative);
        collect_dir(&dir, &root, &mut found);
    }
    let mut counts: BTreeMap<(&str, &str, &str), u32> = BTreeMap::new();
    for hit in &found {
        let function = hit.function.as_deref().unwrap_or("<module>");
        if SANCTIONED.contains(&(hit.file.as_str(), function)) {
            continue;
        }
        *counts
            .entry((hit.file.as_str(), function, hit.call.as_str()))
            .or_insert(0) += 1;
    }

    let mut missing = Vec::new();
    for (file, function, call, times) in CLOSED {
        let got = counts.remove(&(file, function, call)).unwrap_or(0);
        if got != *times {
            missing.push(format!(
                "({file}, {function}, {call}) expected {times}, found {got}"
            ));
        }
    }
    let mut fresh: Vec<String> = counts
        .into_iter()
        .map(|((file, function, call), times)| format!("({file}, {function}, {call}) x{times}"))
        .collect();
    fresh.sort();
    missing.sort();

    assert!(
        missing.is_empty() && fresh.is_empty(),
        "direct vault IO is forbidden.\n\
         Read a vault path through collect_notes or regular_file_inside.\n\
         Confine a user path with canonicalize_within or VaultIndex::resolve_argument.\n\
         Write through with_write_lock / WriteTxn::atomic_write.\n\
         Sanctioned helpers are exempt only in their defining file:\n\
         atomic_write in crates/mnemic-core/src/write/mod.rs,\n\
         canonicalize_within in crates/mnemic-core/src/vault.rs,\n\
         collect_notes in crates/mnemic-core/src/check.rs,\n\
         confine_user_path in crates/mnemic-core/src/index.rs,\n\
         regular_file_inside in crates/mnemic-core/src/check.rs,\n\
         resolve_argument in crates/mnemic-core/src/index.rs,\n\
         with_write_lock in crates/mnemic-core/src/write/mod.rs.\n\
         Do not reimplement, cache, or wrap canonicalize_within.\n\
         New call sites:\n{}\n\
         Closed-set entries that no longer match:\n{}\n",
        if fresh.is_empty() {
            "(none)".to_string()
        } else {
            fresh.join("\n")
        },
        if missing.is_empty() {
            "(none)".to_string()
        } else {
            missing.join("\n")
        },
    );
}

struct Hit {
    file: String,
    call: String,
    function: Option<String>,
}

fn collect_dir(dir: &Path, root: &Path, found: &mut Vec<Hit>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|err| {
        panic!("read {}: {err}", dir.display());
    });
    let mut paths: Vec<PathBuf> = entries
        .map(|entry| entry.expect("dir entry").path())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_dir(&path, root, found);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        found.extend(calls_in(&relative, &mask_non_code(&source)));
    }
}

fn calls_in(relative: &str, masked: &str) -> Vec<Hit> {
    let mut found = Vec::new();
    let mut brace = 0_i32;
    let mut functions: Vec<(String, i32)> = Vec::new();
    let mut pending: Option<String> = None;
    let mut skip: Option<i32> = None;
    let mut arm_cfg = false;
    let mut lookback = Vec::<u8>::new();

    for line in masked.lines() {
        if line.trim_start().starts_with("#[cfg(test)]") {
            arm_cfg = true;
        }
        let bytes = line.as_bytes();
        let mut cursor = 0;
        while cursor < bytes.len() {
            if bytes[cursor..].starts_with(b"fn") && !ident_continues(bytes, cursor) {
                let after = cursor + 2;
                if after < bytes.len() && bytes[after].is_ascii_whitespace() {
                    let name_start = after
                        + 1
                        + bytes[after + 1..]
                            .iter()
                            .take_while(|byte| byte.is_ascii_whitespace())
                            .count();
                    if name_start < bytes.len() && is_ident_start(bytes[name_start]) {
                        let name_end = name_start
                            + bytes[name_start..]
                                .iter()
                                .take_while(|byte| is_ident_continue(**byte))
                                .count();
                        if skip.is_none() {
                            pending = Some(
                                String::from_utf8_lossy(&bytes[name_start..name_end]).into_owned(),
                            );
                        }
                        cursor = name_end;
                        continue;
                    }
                }
            }
            if bytes[cursor] == b'{' {
                brace += 1;
                if let Some(name) = pending.take() {
                    if arm_cfg {
                        skip = Some(brace);
                        arm_cfg = false;
                    } else if skip.is_none() {
                        functions.push((name, brace));
                    }
                } else if arm_cfg {
                    skip = Some(brace);
                    arm_cfg = false;
                }
                cursor += 1;
                continue;
            }
            if bytes[cursor] == b'}' {
                if functions.last().is_some_and(|(_, depth)| *depth == brace) {
                    functions.pop();
                }
                brace -= 1;
                if skip.is_some_and(|depth| brace < depth) {
                    skip = None;
                }
                cursor += 1;
                continue;
            }
            if skip.is_none() {
                if let Some((method, len)) = method_at(bytes, cursor, &lookback) {
                    found.push(Hit {
                        file: relative.to_string(),
                        call: method.to_string(),
                        function: functions.last().map(|(name, _)| name.clone()),
                    });
                    push_lookback(&mut lookback, &bytes[cursor..cursor + len]);
                    cursor += len;
                    continue;
                }
            }
            push_lookback(&mut lookback, &bytes[cursor..cursor + 1]);
            cursor += 1;
        }
        push_lookback(&mut lookback, b"\n");
    }
    found
}

fn push_lookback(lookback: &mut Vec<u8>, bytes: &[u8]) {
    lookback.extend_from_slice(bytes);
    if lookback.len() > 400 {
        let drop_count = lookback.len() - 400;
        lookback.drain(..drop_count);
    }
}

fn method_at(bytes: &[u8], cursor: usize, lookback: &[u8]) -> Option<(&'static str, usize)> {
    const NEEDLES: &[(&str, &str)] = &[
        ("std::fs::read_to_string", "read_to_string"),
        ("std::fs::read_dir", "read_dir"),
        ("std::fs::canonicalize", "canonicalize"),
        ("std::fs::metadata", "metadata"),
        ("std::fs::read", "read"),
        ("fs::read_to_string", "read_to_string"),
        ("fs::read_dir", "read_dir"),
        ("fs::canonicalize", "canonicalize"),
        ("fs::metadata", "metadata"),
        ("fs::read", "read"),
        ("std::fs::File::open", "File::open"),
        ("File::open", "File::open"),
        ("OpenOptions::open", "OpenOptions::open"),
    ];
    if ident_continues(bytes, cursor) {
        return None;
    }
    for (needle, call) in NEEDLES {
        let raw = needle.as_bytes();
        if bytes[cursor..].starts_with(raw) && followed_by_call(bytes, cursor + raw.len()) {
            return Some((call, raw.len()));
        }
    }
    if bytes[cursor..].starts_with(b".open")
        && followed_by_call(bytes, cursor + 5)
        && recent_open_options(lookback, bytes, cursor)
    {
        return Some(("OpenOptions::open", 5));
    }
    None
}

fn followed_by_call(bytes: &[u8], mut index: usize) -> bool {
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index < bytes.len() && bytes[index] == b'('
}

fn recent_open_options(lookback: &[u8], bytes: &[u8], cursor: usize) -> bool {
    let mut window = Vec::with_capacity(lookback.len() + cursor);
    window.extend_from_slice(lookback);
    window.extend_from_slice(&bytes[..cursor]);
    let start = window.len().saturating_sub(240);
    window[start..]
        .windows(11)
        .any(|slice| slice == b"OpenOptions")
}

fn ident_continues(bytes: &[u8], cursor: usize) -> bool {
    cursor > 0 && is_ident_continue(bytes[cursor - 1])
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn mask_non_code(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '/' && chars.peek() == Some(&'/') {
            out.push(' ');
            chars.next();
            out.push(' ');
            for skipped in chars.by_ref() {
                if skipped == '\n' {
                    out.push('\n');
                    break;
                }
                out.push(' ');
            }
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'*') {
            out.push(' ');
            chars.next();
            out.push(' ');
            while let Some(skipped) = chars.next() {
                if skipped == '*' && chars.peek() == Some(&'/') {
                    out.push(' ');
                    chars.next();
                    out.push(' ');
                    break;
                }
                out.push(if skipped == '\n' { '\n' } else { ' ' });
            }
            continue;
        }
        if ch == 'r' && matches!(chars.peek(), Some('#' | '"')) {
            let mut hashes = 0;
            while chars.peek() == Some(&'#') {
                hashes += 1;
                out.push(' ');
                chars.next();
            }
            if chars.peek() == Some(&'"') {
                out.push(' ');
                chars.next();
                out.push(' ');
                let mut matched = 0;
                for skipped in chars.by_ref() {
                    if skipped == '"' && matched == 0 {
                        matched = 1;
                    } else if skipped == '#' && matched > 0 {
                        matched += 1;
                        if matched == hashes + 1 {
                            out.push(' ');
                            break;
                        }
                    } else {
                        matched = 0;
                    }
                    out.push(if skipped == '\n' { '\n' } else { ' ' });
                }
                continue;
            }
        }
        if ch == '"' {
            out.push(' ');
            let mut escaped = false;
            for skipped in chars.by_ref() {
                if skipped == '\n' {
                    out.push('\n');
                } else {
                    out.push(' ');
                }
                if escaped {
                    escaped = false;
                    continue;
                }
                if skipped == '\\' {
                    escaped = true;
                    continue;
                }
                if skipped == '"' {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}
