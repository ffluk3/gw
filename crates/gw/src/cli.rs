use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "gw",
    version,
    about = "Manage git worktrees in a bare-repo layout",
    after_help = "WHERE TO RUN GW\n  \
        gw works inside a bare-repo layout:\n\
        \n\
        \x20   <root>/.bare/               the bare repository\n\
        \x20   <root>/worktrees/<branch>/  one checkout per branch\n\
        \n\
        \x20 Run it from any worktree, from <root>/.bare, or from <root> itself.\n\
        \x20 gw walks up from the current directory, so a subdirectory works too.\n\
        \n\
        \x20 Everywhere else it stops and tells you the two ways forward:\n\
        \x20   gw --repo <root> <command>     point at an existing layout\n\
        \x20   gw init <remote> --dir <root>  create a new one\n\
        \n\
        \x20 'gw init' is the exception: it creates a layout and needs no existing one.\n\
        \n\
        \x20 --repo also accepts an owner/name (or unique name) resolved under\n\
        \x20 GW_REPOS_ROOT (default ~/repos), so a command works from anywhere:\n\
        \x20   gw --repo owner/name status\n\
        \n\
        \x20 GW_REPO=<root> is the environment equivalent of --repo.",
    long_about = "Gw manages one worktree per branch under <root>/worktrees, backed by a bare \
                  repository at <root>/.bare.\n\n\
                  It discovers that layout from the directory you run it in, so it works \
                  unchanged in any project that follows the convention. Nothing about a \
                  particular repository, host, or ticket system is built in."
)]
pub struct Cli {
    /// Repository path, or an owner/name resolved under the repos root
    /// (default: discovered from cwd).
    #[arg(long, global = true, env = "GW_REPO", value_name = "PATH|SLUG")]
    pub repo: Option<String>,

    /// Force machine-readable output.
    #[arg(long, global = true)]
    pub json: bool,

    /// Strip decorative emoji from human output.
    ///
    /// JSON documents and table content, including the `●` and `—` markers,
    /// are unaffected. `NO_COLOR` set to any non-empty value has the same
    /// effect; an empty value does not.
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Do not query this provider. Repeatable.
    ///
    /// Providers are named by the rules in `.gw.toml`; the built-in ones are
    /// `jira` and `github`.
    #[arg(
        long = "skip-provider",
        global = true,
        env = "GW_SKIP_PROVIDERS",
        value_delimiter = ',',
        value_name = "ID"
    )]
    pub skip_provider: Vec<String>,

    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    /// Whether decorative output is disabled for this run.
    ///
    /// `--no-color` always disables. `NO_COLOR` disables when it is present and
    /// non-empty, whatever the value: [no-color.org][spec] treats even `0` as
    /// "set". An empty or unset value does not disable.
    ///
    /// This is read outside clap deliberately. Binding `NO_COLOR` to a `bool`
    /// arg makes clap parse the value as a boolean, so `NO_COLOR=1` and
    /// `NO_COLOR=0` become usage errors instead of a request to disable
    /// decoration.
    ///
    /// [spec]: https://no-color.org/
    #[must_use]
    pub fn color_disabled(&self) -> bool {
        self.no_color || std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a bare-repo worktree layout for a project.
    ///
    /// Unlike every other command, this one does not need to run inside an
    /// existing layout: it is how you make one.
    Init(InitArgs),
    /// Show every worktree with its age, dirtiness, and ticket summary.
    Status(StatusArgs),
    /// Create or reuse a worktree for a branch.
    Add(AddArgs),
    /// Remove stale and merged worktrees, and delete stale orphan branches.
    Clean(CleanArgs),
    /// List the layouts under the repos root.
    Repos(ReposArgs),
    /// Inspect the convention that maps branches to tickets and changes.
    Convention(ConventionArgs),
}

#[derive(Debug, Args)]
pub struct ConventionArgs {
    #[command(subcommand)]
    pub action: ConventionAction,
}

#[derive(Debug, Subcommand)]
pub enum ConventionAction {
    /// Show the effective convention and which providers are active.
    Show,
    /// Show what each rule does with one branch, and why.
    Explain {
        /// Branch name to resolve.
        branch: String,
    },
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Repository to clone, as any URL or path git accepts.
    ///
    /// This command does not require you to be inside an existing layout.
    #[arg(value_name = "REMOTE")]
    pub remote: String,

    /// Layout root to create. Defaults to ./<repository-name>.
    #[arg(long, value_name = "DIR")]
    pub dir: Option<PathBuf>,

    /// Branch to check out first. Defaults to the remote's HEAD.
    #[arg(long, value_name = "BRANCH")]
    pub branch: Option<String>,

    /// Describe what would be created without writing anything.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct StatusArgs {}

#[derive(Debug, Args)]
pub struct AddArgs {
    /// Branch to check out. A new branch is created if it does not exist.
    pub branch: String,

    /// Print the worktree path and nothing else. The shell wrapper uses this
    /// to `cd` without the binary mutating the caller's directory.
    #[arg(long)]
    pub print_path: bool,

    /// Change the calling shell into the new worktree.
    ///
    /// With the shell integration sourced this is already the default, so the
    /// flag is only needed to override `GW_CD=0`. A process cannot change its
    /// parent shell's directory, so the shell function acts on this flag. Run
    /// standalone, `gw` prints the path on stdout and warns on stderr that it
    /// cannot change your shell's directory.
    #[arg(long, overrides_with = "no_cd")]
    pub cd: bool,

    /// Stay in the current directory instead of entering the new worktree.
    ///
    /// This wins over the default, over `--cd`, and over `GW_CD`.
    #[arg(long, overrides_with = "cd")]
    pub no_cd: bool,

    /// Do not fetch from origin first.
    #[arg(long)]
    pub no_fetch: bool,
}

#[derive(Debug, Args)]
pub struct ReposArgs {
    /// Also count worktrees with uncommitted changes. This runs one
    /// `git status` per worktree, so it is opt-in.
    #[arg(long)]
    pub dirty: bool,
}

#[derive(Debug, Args)]
pub struct CleanArgs {
    /// Plan only; change nothing.
    ///
    /// This is also the default when stdout is not a TTY, and when --json is
    /// given. Only --yes applies a plan in those cases.
    #[arg(long)]
    pub dry_run: bool,

    /// Apply the plan without prompting.
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Also remove worktrees whose JIRA ticket is closed.
    #[arg(long)]
    pub closed: bool,

    /// Also delete stale orphan local branches whose commits are all published.
    #[arg(long)]
    pub delete_branches: bool,

    /// Choose which worktrees to act on from a list.
    ///
    /// Overrides policy only: you can pick a worktree whose ticket is closed
    /// without --closed, but never one that is dirty, unpublished, protected,
    /// or otherwise kept. Needs a terminal on stdin and stdout. Conflicts with
    /// --yes, and is a usage error with --json or --delete-branches.
    #[arg(long, short = 'i', conflicts_with = "yes")]
    pub interactive: bool,

    /// Days since the last commit before a worktree is stale.
    ///
    /// When absent, `[clean] stale_after` from `.gw.toml` applies, and when
    /// that is absent too the built-in default is 7 days.
    #[arg(long = "stale-days", env = "GW_STALE_DAYS")]
    pub stale_days: Option<i64>,
}
