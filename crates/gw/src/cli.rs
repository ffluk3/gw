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

    /// Disable color and emoji.
    #[arg(long, global = true, env = "NO_COLOR")]
    pub no_color: bool,

    /// JIRA project key(s) a branch name may reference. Comma-separated.
    #[arg(
        long = "jira-project",
        global = true,
        env = "GW_JIRA_PROJECT",
        value_delimiter = ',',
        value_name = "KEY"
    )]
    pub jira_project: Vec<String>,

    /// JIRA base URL. Overrides the value in `.gw.toml`.
    #[arg(
        long = "jira-url",
        global = true,
        env = "GW_JIRA_BASE_URL",
        value_name = "URL"
    )]
    pub jira_url: Option<String>,

    /// Do not query this provider. Repeatable. Providers are named in
    /// `.gw.toml` rules; the built-in ones are `jira` and `github`.
    #[arg(
        long = "skip-provider",
        global = true,
        env = "GW_SKIP_PROVIDERS",
        value_delimiter = ',',
        value_name = "ID"
    )]
    pub skip_provider: Vec<String>,

    /// Skip the code-host change lookup. Same as `--skip-provider github`.
    ///
    /// `GW_SKIP_GITHUB` is read as a presence flag, so `GW_SKIP_GITHUB=1` and
    /// `GW_SKIP_GITHUB=0` both mean "skip" — matching how the variable behaved
    /// before this flag existed.
    #[arg(long, global = true)]
    pub skip_github: bool,

    #[command(subcommand)]
    pub command: Command,
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
pub struct StatusArgs {
    /// Skip the JIRA summary lookup.
    #[arg(long)]
    pub no_jira: bool,
}

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
    /// A process cannot change its parent shell's directory, so the shell
    /// integration acts on this flag. Run standalone, `gw` prints the path
    /// and nothing else, exactly like `--print-path`.
    #[arg(long, overrides_with = "no_cd")]
    pub cd: bool,

    /// Stay in the current directory, even when the shell integration or
    /// `GW_CD=1` would otherwise change into the worktree.
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

    /// Skip the JIRA lookup.
    #[arg(long)]
    pub no_jira: bool,

    /// Days since the last commit before a worktree is stale.
    #[arg(long = "stale-days", env = "GW_STALE_DAYS", default_value_t = 7)]
    pub stale_days: i64,
}
