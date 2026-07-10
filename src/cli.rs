use clap::{Parser, Subcommand, ValueEnum, Args};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(long)]
    pub db: Option<String>,

    #[arg(long)]
    pub workspace: Option<String>,

    #[arg(long, value_enum, default_value_t = Format::Json)]
    pub format: Format,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Json,
    Human,
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
    
    /// Manage the active context document
    ActiveContext {
        #[command(subcommand)]
        cmd: ContextCmd,
    },

    /// View history of context documents
    History {
        doc: HistoryDoc,
        #[arg(long)]
        version: Option<i64>,
        #[arg(long, default_value_t = 50)]
        limit: i64,
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
    /// Log a new architectural decision
    Log {
        /// Short summary of the decision
        #[arg(long)]
        summary: String,
        /// Detailed reasoning behind the decision
        #[arg(long)]
        rationale: Option<String>,
        /// Specific implementation details
        #[arg(long)]
        details: Option<String>,
        /// Comma-separated list of tags
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
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
    /// Full-text search across decisions
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: i64,
    },
    /// Update fields of an existing decision
    Update(DecisionUpdateArgs),
    /// Delete a decision and its links
    Delete {
        id: i64,
    },
}

#[derive(Args, Debug)]
pub struct DecisionUpdateArgs {
    pub id: i64,

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
}

#[derive(Subcommand, Debug)]
pub enum ProgressCmd {
    /// Log task execution and progress
    Log {
        /// Current status (e.g. InProgress, Done)
        #[arg(long)]
        status: String,
        /// What was done or is currently happening
        #[arg(long)]
        description: String,
        /// ID of the parent progress entry
        #[arg(long)]
        parent_id: Option<i64>,
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

    #[command(flatten)]
    pub fields: ProgressUpdateFields,
}

#[derive(Args, Debug)]
#[group(required = true, multiple = true)]
pub struct ProgressUpdateFields {
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
