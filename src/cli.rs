use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(long)]
    pub db: Option<String>,

    #[arg(long)]
    pub workspace: Option<String>,

    #[arg(long, global = true)]
    pub compact: bool,

    #[arg(long, global = true, value_delimiter = ',')]
    pub fields: Vec<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Initialize the database schema
    Init,

    /// Migrate the database schema to the latest version
    Migrate,
    /// Manage the product context document
    ProductContext {
        #[command(subcommand)]
        cmd: ContextCmd,
    },

    /// Manage named active-context tracks
    ActiveContext {
        #[command(subcommand)]
        cmd: ActiveContextCmd,
    },

    /// View history of context documents
    History {
        doc: HistoryDoc,
        #[arg(long)]
        version: Option<i64>,
        #[arg(long, default_value_t = 50)]
        limit: i64,
        /// Active-context track name (only for active-context)
        #[arg(long)]
        name: Option<String>,
        /// List version counts for every active-context track
        #[arg(long)]
        all: bool,
    },

    /// Log and search architectural decisions
    Decision {
        #[command(subcommand)]
        cmd: DecisionCmd,
    },

    /// Track task execution and progress
    Progress {
        #[command(subcommand)]
        cmd: ProgressCmd,
    },

    /// Log recurring system patterns and conventions
    Pattern {
        #[command(subcommand)]
        cmd: PatternCmd,
    },

    /// Export checkable patterns as harness rule files (omp rulebook)
    Rules {
        #[command(subcommand)]
        cmd: RulesCmd,
    },

    /// Run stored checks against files (CI / session-end review)
    Check {
        /// Scan git-staged files only
        #[arg(long)]
        staged: bool,
        /// Comma-separated list of files/dirs to scan
        #[arg(long, value_delimiter = ',')]
        paths: Vec<String>,
    },

    /// Install harness rule files into the workspace for in-session enforcement
    Install {
        /// Target harness (only 'omp' supported)
        #[arg(long)]
        harness: String,
        /// Also install a git pre-commit hook running `engrams check --staged`
        #[arg(long)]
        hooks: bool,
    },

    /// Store arbitrary configuration or key-value data
    Custom {
        #[command(subcommand)]
        cmd: CustomCmd,
    },

    /// Create knowledge-graph relations between items
    Link {
        #[command(subcommand)]
        cmd: LinkCmd,
    },

    /// Get a recent summary of all modifications
    Activity(ActivityArgs),

    /// Generate a structured report of project knowledge
    Report {
        #[command(subcommand)]
        cmd: Option<ReportCmd>,
        /// Show only a specific topic
        topic: Option<ReportTopic>,
        /// Max items per section
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },
    /// Generate a context brief optimized for token budget
    Prime {
        /// Approximate token budget for the briefing
        #[arg(long)]
        budget: Option<usize>,
        /// Filter by specific anchor paths
        #[arg(long, value_delimiter = ',')]
        paths: Vec<String>,
        /// Filter by tags
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },

    /// Analyze project database for orphaned records, drift, and missing context
    Doctor,

    /// Compute and query the knowledge + code graph
    Graph {
        #[command(subcommand)]
        cmd: GraphCmd,
    },

    /// Print onboarding instructions for LLM agents
    Instructions,

    /// Search across decisions, patterns, and custom data
    Query {
        query: String,
        /// Restrict to types (default: all three)
        #[arg(long, value_delimiter = ',')]
        types: Vec<QueryType>,
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        /// RFC3339 lower bound on timestamp
        #[arg(long)]
        since: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: i64,
        /// Include superseded decisions
        #[arg(long)]
        all: bool,
    },

    /// Perform multiple operations in a single transaction
    Batch {
        #[arg(long)]
        r#type: BatchType,
        #[arg(long)]
        items: String,
    },

    /// Dump database to Markdown files for git sync
    Export {
        #[arg(long, default_value = "./engrams_export")]
        path: std::path::PathBuf,
    },

    /// Import Markdown files back into the database
    Import {
        #[arg(long, default_value = "./engrams_export")]
        path: std::path::PathBuf,
    },

    /// Manage PR URLs/numbers associated with decisions and patterns
    Pr {
        #[command(subcommand)]
        cmd: PrCmd,
    },

    /// Manage file path anchors for decisions and patterns
    Anchor {
        #[command(subcommand)]
        cmd: AnchorCmd,
    },

    /// Find decisions and patterns relevant to path(s)
    Relevant {
        /// Paths to match against stored anchors
        paths: Vec<String>,
        /// Use git diff --cached --name-only as the path list
        #[arg(long)]
        staged: bool,
        /// Include superseded decisions
        #[arg(long)]
        all: bool,
    },
    /// Pre-edit advisory: constraints and violations for the given paths
    Advise {
        /// Paths to check for anchored constraints
        paths: Vec<String>,
        /// Use git diff --cached --name-only as the path list
        #[arg(long)]
        staged: bool,
    },

    /// Archive decayed decisions and patterns (prune-decay)
    Prune(PruneCmd),
}

#[derive(Args, Debug)]
pub struct PruneCmd {
    /// Retention threshold below which items are pruned (default: 0.1)
    #[arg(long)]
    pub threshold: Option<f64>,
    /// Show what would be pruned without archiving
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Subcommand, Debug)]
pub enum GraphCmd {
    /// Recompute all derived edges (anchors, co-anchor/tag overlap, git co-change)
    Rebuild {
        /// Skip git co-change ingest
        #[arg(long)]
        no_git: bool,
        /// Minimum co-change count for a co_changes edge
        #[arg(long, default_value_t = 2)]
        min_cochange: i64,
        /// Maximum commits scanned during ingest
        #[arg(long, default_value_t = 500)]
        max_commits: i64,
    },
    /// Incrementally ingest git co-change edges (resumes from last ingested commit)
    Ingest {
        /// Commit SHA to start from (default: last ingested commit)
        #[arg(long)]
        since: Option<String>,
        /// Maximum commits scanned
        #[arg(long, default_value_t = 500)]
        max_commits: i64,
        /// Minimum co-change count for a co_changes edge
        #[arg(long, default_value_t = 2)]
        min_cochange: i64,
    },
    /// Node/edge counts, density, components, orphans, degree stats
    Stats,
    /// PageRank centrality ranking
    Central {
        #[arg(long, default_value_t = 10)]
        limit: i64,
        /// Restrict to one node type
        #[arg(long = "type")]
        node_type: Option<String>,
    },
    /// Connected components as clusters
    Clusters {
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    /// Nodes with degree <= 1
    Orphans {
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },
    /// Shortest path between two nodes (type:id)
    Path {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    /// Nodes within N hops of a node (type:id)
    Neighbors {
        #[arg(long)]
        node: String,
        #[arg(long, default_value_t = 1)]
        depth: i64,
        /// Restrict traversal to one relationship type
        #[arg(long)]
        rel: Option<String>,
    },
    /// Transitive closure over a canonical transitive rel from a node
    /// ('what breaks if I revisit X')
    Chain {
        /// Start node (type:id); alternative to --item-type/--item-id
        #[arg(long, conflicts_with_all = ["item_type", "item_id"])]
        node: Option<String>,
        /// Start node item type (requires --item-id)
        #[arg(long, requires = "item_id")]
        item_type: Option<String>,
        /// Start node item id (requires --item-type)
        #[arg(long, requires = "item_type")]
        item_id: Option<String>,
        /// Canonical transitive relationship (supersedes, depends_on, part_of, refines)
        #[arg(long)]
        rel: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ReportCmd {
    /// Generate and open a browser-based HTML dashboard with the knowledge graph
    Open {
        /// Write the HTML file without launching a browser
        #[arg(long)]
        no_browser: bool,
        /// Output path for the generated HTML (defaults to the system temp dir)
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ReportTopic {
    Context,
    Progress,
    Decisions,
    Patterns,
    Links,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum HistoryDoc {
    ProductContext,
    ActiveContext,
}

#[derive(Subcommand, Debug)]
pub enum ContextCmd {
    /// Retrieve the current context document
    Get,
    /// Update or patch the context document
    Update(ContextUpdateArgs),
}

#[derive(Subcommand, Debug)]
pub enum ActiveContextCmd {
    /// Retrieve an active-context track
    Get {
        /// Track name
        #[arg(long, default_value = "default")]
        name: String,
    },
    /// Update or patch an active-context track
    Update {
        /// Track name
        #[arg(long, default_value = "default")]
        name: String,
        #[command(flatten)]
        args: ContextUpdateArgs,
    },
    /// List all active-context tracks
    List,
}

#[derive(Args, Debug)]
#[group(required = true, multiple = false)]
pub struct ContextUpdateArgs {
    /// Full JSON content to replace the document
    #[arg(long)]
    pub content: Option<String>,
    /// JSON object to merge (use \"__DELETE__\" to remove keys)
    #[arg(long)]
    pub patch: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum DecisionCmd {
    /// Log a new architectural decision (checks for similar existing decisions by default)
    Log {
        /// Short summary of the decision
        #[arg(long)]
        summary: String,
        /// Initial status (default: active; valid: active, superseded, rejected, revisited)
        #[arg(long)]
        status: Option<String>,
        /// Detailed reasoning behind the decision
        #[arg(long)]
        rationale: Option<String>,
        /// Specific implementation details
        #[arg(long)]
        details: Option<String>,
        /// Comma-separated list of tags
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        /// Skip similarity check and insert unconditionally
        #[arg(long)]
        force: bool,
        /// Associated PR number or URL (repeatable)
        #[arg(long = "pr")]
        prs: Vec<String>,
        /// Associated file path anchor (repeatable)
        #[arg(long = "anchor")]
        anchors: Vec<String>,
        /// Importance weight 0-10 for retrieval scoring (default: 5)
        #[arg(long)]
        importance: Option<i64>,
    },
    /// List decisions, optionally filtering by tags
    List {
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        #[arg(long, default_value_t = 20)]
        limit: i64,
        /// Include superseded decisions
        #[arg(long)]
        all: bool,
    },
    /// Get a specific decision by ID
    Get { id: i64 },
    /// Full-text search across decisions
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: i64,
        /// Include superseded decisions
        #[arg(long)]
        all: bool,
        /// Enable snippets with FTS highlights
        #[arg(long)]
        snippets: bool,
    },
    /// Update fields of an existing decision
    Update(DecisionUpdateArgs),
    /// Delete a decision and its links
    Delete { id: i64 },
    /// Merge source decision into target, combining rationale/details/tags and repointing links
    Consolidate {
        /// ID of the decision to merge away (will be deleted)
        source_id: i64,
        /// ID of the decision to merge into (will be updated)
        into_id: i64,
    },
    /// Supersede a decision with another
    Supersede {
        id: i64,
        /// ID of the decision that supersedes this one
        #[arg(long)]
        by: Option<i64>,
    },
}
#[derive(Args, Debug)]
pub struct DecisionUpdateArgs {
    pub id: i64,

    /// Bypass status_vocabulary validation
    #[arg(long)]
    pub force: bool,

    #[command(flatten)]
    pub fields: DecisionUpdateFields,
}

#[derive(Args, Debug)]
#[group(required = true, multiple = true)]
pub struct DecisionUpdateFields {
    /// New summary
    #[arg(long)]
    pub summary: Option<String>,
    /// New rationale
    #[arg(long)]
    pub rationale: Option<String>,
    /// New details
    #[arg(long)]
    pub details: Option<String>,
    /// New tags (replaces existing tags)
    #[arg(long, value_delimiter = ',')]
    pub tags: Option<Vec<String>>,
    /// New importance weight 0-10
    #[arg(long)]
    pub importance: Option<i64>,
    /// New status (active, superseded, rejected, revisited)
    #[arg(long)]
    pub status: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum ProgressCmd {
    /// Log task execution and progress
    Log {
        /// Current status (valid: Todo, InProgress, InReview, Blocked, Done, Dropped)
        #[arg(long)]
        status: String,
        /// What was done or is currently happening
        #[arg(long)]
        description: String,
        /// ID of the parent progress entry
        #[arg(long)]
        parent_id: Option<i64>,
        /// Check for recent entries with similar descriptions before inserting
        #[arg(long)]
        check_similar: bool,
        /// Bypass status_vocabulary validation
        #[arg(long)]
        force: bool,
    },
    /// List progress entries
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        parent_id: Option<i64>,
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    /// Get a specific decision by ID
    Get {
        id: i64,
    },
    Update(ProgressUpdateArgs),
    Delete {
        id: i64,
    },
}

#[derive(Args, Debug)]
pub struct ProgressUpdateArgs {
    pub id: i64,

    /// Bypass status_vocabulary validation
    #[arg(long)]
    pub force: bool,

    #[command(flatten)]
    pub fields: ProgressUpdateFields,
}

#[derive(Args, Debug)]
#[group(required = true, multiple = true)]
pub struct ProgressUpdateFields {
    /// New status (valid: Todo, InProgress, InReview, Blocked, Done, Dropped)
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub parent_id: Option<i64>,
}

#[derive(Subcommand, Debug)]
pub enum PatternCmd {
    /// Log a recurring system pattern or convention
    Log {
        /// Unique name of the pattern
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        /// Comma-separated list of tags
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        /// Associated PR number or URL (repeatable)
        #[arg(long = "pr")]
        prs: Vec<String>,
        /// Associated file path anchor (repeatable)
        #[arg(long = "anchor")]
        anchors: Vec<String>,
        /// Machine-checkable expression kind: regex or ast
        #[arg(long)]
        check_kind: Option<String>,
        /// Check expression source (regex pattern or ast-grep pattern); pairs with --check-kind
        #[arg(long = "check")]
        check_expr: Option<String>,
        /// Enforcement severity when a check matches: info, warn, or error (default: warn)
        #[arg(long)]
        severity: Option<String>,
        /// Importance weight 0-10 for retrieval scoring (default: 5)
        #[arg(long)]
        importance: Option<i64>,
    },
    /// List decisions, optionally filtering by tags
    List {
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    /// Get a specific decision by ID
    Get {
        id: i64,
    },
    Delete {
        id: i64,
    },
}

#[derive(Subcommand, Debug)]
pub enum RulesCmd {
    /// Export checkable patterns as harness rule files
    Export {
        /// Harness target (only "omp" supported)
        #[arg(long)]
        harness: String,
        /// Output directory (defaults to <workspace>/.omp/rules)
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum CustomCmd {
    /// Set a custom configuration or key-value pair
    Set {
        /// Category grouping the data
        #[arg(long)]
        category: String,
        /// Unique key within the category
        #[arg(long)]
        key: String,
        /// Data value (string or JSON)
        #[arg(long)]
        value: String,
        #[arg(long)]
        json: bool,
    },
    /// Get a specific decision by ID
    Get {
        #[arg(long)]
        category: Option<String>,
        #[arg(long, requires = "category")]
        key: Option<String>,
    },
    /// Full-text search across decisions
    Search {
        query: String,
        #[arg(long)]
        category: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: i64,
        /// Enable snippets with FTS highlights
        #[arg(long)]
        snippets: bool,
    },
    Delete {
        /// Category grouping the data
        #[arg(long)]
        category: String,
        /// Unique key within the category
        #[arg(long)]
        key: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum LinkCmd {
    /// Create a relationship between two items
    Add {
        #[arg(long)]
        source_type: ItemType,
        #[arg(long)]
        source_id: String,
        #[arg(long)]
        target_type: ItemType,
        #[arg(long)]
        target_id: String,
        #[arg(long)]
        rel: String,
        #[arg(long)]
        description: Option<String>,
        /// Bypass relationship constraint validation (violations reported as overrides)
        #[arg(long)]
        force: bool,
    },
    /// List progress entries
    List {
        #[arg(long)]
        item_type: ItemType,
        #[arg(long)]
        item_id: String,
        #[arg(long)]
        rel: Option<String>,
        #[arg(long)]
        linked_type: Option<ItemType>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ItemType {
    Decision,
    ProgressEntry,
    SystemPattern,
    CustomData,
}

impl ItemType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ItemType::Decision => "decision",
            ItemType::ProgressEntry => "progress_entry",
            ItemType::SystemPattern => "system_pattern",
            ItemType::CustomData => "custom_data",
        }
    }

    pub fn table_name(&self) -> &'static str {
        match self {
            ItemType::Decision => "decisions",
            ItemType::ProgressEntry => "progress_entries",
            ItemType::SystemPattern => "system_patterns",
            ItemType::CustomData => "custom_data",
        }
    }
}

#[derive(Args, Debug)]
#[group(multiple = false)]
pub struct ActivityArgs {
    /// Number of hours to look back
    #[arg(long, default_value_t = 24)]
    pub hours: i64,
    /// Explicit RFC3339 cutoff timestamp
    #[arg(long)]
    pub since: Option<String>,
    /// Maximum items to return per category
    #[arg(long, default_value_t = 5)]
    pub limit_per_type: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum BatchType {
    Decision,
    Progress,
    Pattern,
    CustomData,
}

#[derive(Subcommand, Debug)]
pub enum PrCmd {
    /// Attach PR URL or number to a decision or pattern
    Add {
        #[arg(long = "type")]
        item_type: RefItemType,
        #[arg(long)]
        id: i64,
        #[arg(long = "pr", required = true)]
        prs: Vec<String>,
    },
    /// List PR URLs attached to an item
    List {
        #[arg(long = "type")]
        item_type: RefItemType,
        #[arg(long)]
        id: i64,
    },
    /// Remove a PR URL from an item
    Remove {
        #[arg(long = "type")]
        item_type: RefItemType,
        #[arg(long)]
        id: i64,
        #[arg(long)]
        url: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum AnchorCmd {
    /// Attach file path anchors to a decision or pattern
    Add {
        #[arg(long = "type")]
        item_type: RefItemType,
        #[arg(long)]
        id: i64,
        #[arg(long = "path", required = true)]
        paths: Vec<String>,
    },
    /// List file anchors attached to an item
    List {
        #[arg(long = "type")]
        item_type: RefItemType,
        #[arg(long)]
        id: i64,
    },
    /// Remove a file anchor from an item
    Remove {
        #[arg(long = "type")]
        item_type: RefItemType,
        #[arg(long)]
        id: i64,
        #[arg(long)]
        path: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum RefItemType {
    Decision,
    SystemPattern,
}

impl RefItemType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RefItemType::Decision => "decision",
            RefItemType::SystemPattern => "system_pattern",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum QueryType {
    Decision,
    Pattern,
    Custom,
}
