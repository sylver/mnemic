use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mnemic", version, about = "Mnemic")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
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
        /// Insert a missing id, and a missing type when the folder maps to one type.
        #[arg(long)]
        fix: bool,
    },
    /// Scaffold a vault in the target directory.
    Init {
        /// Print the JSON envelope.
        #[arg(long)]
        json: bool,
        /// Target directory. Does not walk to a parent vault.
        #[arg(long)]
        vault: Option<PathBuf>,
        /// Actor written into `mnemic.toml` when that file is created.
        #[arg(long = "as")]
        actor: Option<String>,
    },
    /// Append one timeline line to a note.
    Log {
        /// Path, id, slug, or alias.
        note: String,
        /// `YYYY-MM-DD | source | summary` or `YYYY-MM-DD | summary`.
        text: String,
        /// Print the JSON envelope.
        #[arg(long)]
        json: bool,
        /// Vault root. Defaults to the nearest parent `mnemic.toml`.
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Search parsed notes. Tokens are literal and combined with AND.
    Search {
        /// Whitespace-separated tokens. Every token must match.
        query: Option<String>,
        /// Exact type name.
        #[arg(long = "type")]
        type_name: Option<String>,
        /// Tag, or a prefix path such as `work` for `work/x`.
        #[arg(long)]
        tag: Option<String>,
        /// Inclusive lower bound on `updated` (ISO date or datetime).
        #[arg(long)]
        since: Option<String>,
        /// Maximum hits. Default 20. Zero or less is a usage error.
        #[arg(long)]
        limit: Option<i64>,
        /// Print the JSON envelope.
        #[arg(long)]
        json: bool,
        /// Vault root. Defaults to the nearest parent `mnemic.toml`.
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Print one note by id, slug, alias, or vault-relative path.
    Get {
        /// Id, slug, alias, or vault-relative path.
        query: String,
        /// Print the JSON envelope.
        #[arg(long)]
        json: bool,
        /// Vault root. Defaults to the nearest parent `mnemic.toml`.
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// List outgoing links, backlinks, and broken links for one note.
    Links {
        /// Id, slug, alias, or vault-relative path.
        note: String,
        /// Print the JSON envelope.
        #[arg(long)]
        json: bool,
        /// Vault root. Defaults to the nearest parent `mnemic.toml`.
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Create a note from a type template.
    New {
        /// Type name in the loaded pack.
        type_name: String,
        /// Note title. The filename slug is derived from it.
        title: String,
        /// Crockford ULID. Generated when omitted.
        #[arg(long)]
        id: Option<String>,
        /// Actor stored in `created_by`. Overrides `actor` in `mnemic.toml`.
        #[arg(long = "as")]
        actor: Option<String>,
        /// Journal `date` as `YYYY-MM-DD`. Defaults to the process local date.
        #[arg(long)]
        date: Option<String>,
        /// Print the JSON envelope.
        #[arg(long)]
        json: bool,
        /// Vault root. Defaults to the nearest parent `mnemic.toml`.
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Move or rename a note and rewrite incoming wikilinks.
    Mv {
        /// Path, id, slug, or alias.
        note: String,
        /// Vault-relative destination file or directory.
        dest: PathBuf,
        /// Keep incoming wikilinks unchanged.
        #[arg(long)]
        no_rewrite: bool,
        /// Print the JSON envelope.
        #[arg(long)]
        json: bool,
        /// Vault root. Defaults to the nearest parent `mnemic.toml`.
        #[arg(long)]
        vault: Option<PathBuf>,
    },
}
