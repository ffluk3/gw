use gw_core::classify::add::{add_strategy, AddStrategy};
use gw_core::BranchName;

use crate::cli::AddArgs;
use crate::context::Context;
use crate::error::Error;
use crate::git::{AddMode, GitBackend, TrackStart};
use crate::output::Outcome;

pub fn run(ctx: &Context, args: AddArgs) -> Result<Outcome, Error> {
    let branch = BranchName::parse(&args.branch)
        .map_err(|e| Error::Usage(format!("invalid branch name '{}': {e}", args.branch)))?;

    let path = ctx.layout.worktree_path(&branch);

    if !args.no_fetch {
        eprintln!("🔄 Fetching latest refs...");
        if let Err(e) = ctx.git.fetch_prune() {
            eprintln!("warning: fetch failed, using cached refs ({e})");
        }
    }

    let existing = ctx
        .git
        .worktree_list()?
        .into_iter()
        .find(|entry| entry.branch.as_ref() == Some(&branch))
        .map(|entry| entry.path);

    let has_local = ctx.git.ref_exists(&branch)?;
    let has_remote = ctx.git.remote_ref_exists(&branch)?;

    let strategy = add_strategy(existing, has_local, has_remote);
    let created = !matches!(strategy, AddStrategy::UseExisting { .. });

    match strategy {
        AddStrategy::UseExisting { path } => {
            eprintln!(
                "⚡ Branch '{branch}' already has a worktree at {}",
                path.display()
            );
        }
        AddStrategy::FastForwardThenUse => {
            if has_remote {
                ctx.git.fast_forward_to_remote(&branch).map_err(|e| {
                    Error::Usage(format!(
                        "cannot fast-forward '{branch}' to origin/{branch}: {e}\n\
                         If it is checked out in another worktree, move that worktree first."
                    ))
                })?;
            }
            ctx.git.worktree_add(&path, &branch, AddMode::Existing)?;
        }
        AddStrategy::TrackRemote => {
            ctx.git.worktree_add(
                &path,
                &branch,
                AddMode::Track {
                    start: TrackStart::Remote,
                },
            )?;
        }
        AddStrategy::CreateNew => {
            ctx.git.worktree_add(
                &path,
                &branch,
                AddMode::Track {
                    start: TrackStart::Head,
                },
            )?;
        }
    }

    let path = ctx.layout.worktree_path(&branch);
    if args.print_path || args.cd {
        // A bare path is a stricter contract than JSON ("the path and nothing
        // else"); the shell wrapper depends on it, so it wins over --json.
        //
        // --cd means the same thing to the binary: it cannot change the
        // caller's directory, so it hands the path to the shell function that
        // performs the `cd`.
        println!("{}", path.display());
    } else if ctx.is_json() {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "branch": branch.as_str(),
                "path": path.display().to_string(),
                "created": created,
            }))
            .unwrap_or_else(|_| "{}".into())
        );
    } else {
        println!("✅ Worktree ready at {}", path.display());
    }
    Ok(Outcome::ok())
}
