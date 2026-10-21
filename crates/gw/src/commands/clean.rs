use std::collections::HashSet;
use std::io::IsTerminal;

use gw_core::render::table::render_table;
use gw_core::{
    classify::classify_clean, decision::Decision, decision::Target, render::human::restore_hint,
    Layout, Plan, Policy,
};

use crate::cli::CleanArgs;
use crate::config::{now_epoch, repo_slug};
use crate::context::Context;
use crate::error::Error;
use crate::git::GitBackend;
use crate::github::{GhCli, GithubBackend, NoGithub};
use crate::jira::{JiraBackend, JiraRest};
use crate::output::Outcome;

use super::snapshot;

pub fn run(ctx: &Context, args: CleanArgs) -> Result<Outcome, Error> {
    let policy = Policy {
        stale_after_secs: args.stale_days.saturating_mul(86_400),
        clean_closed_tickets: args.closed,
        delete_branches: args.delete_branches,
        dry_run: args.dry_run,
    };

    let now = now_epoch();
    println!("🔍 Scanning worktrees in {}", ctx.layout.root.display());

    // Refresh remote refs so "unpublished" is judged against current origin.
    if let Err(e) = ctx.git.fetch_prune() {
        eprintln!("warning: could not fetch, judging against cached refs ({e})");
    }

    let mut snap = snapshot::load(ctx, now)?;

    let merged = if args.skip_github {
        HashSet::new()
    } else {
        attach_github(ctx, &mut snap)?
    };
    snapshot::attach_merged(&mut snap, &merged);

    if !args.no_jira {
        attach_jira(ctx, &mut snap);
    }

    let plan = classify_clean(&snap, &policy);

    let apply = should_apply(&args, &plan);
    if ctx.is_json() {
        println!("{}", gw_core::render::json::to_json_string(&plan));
    } else if plan.is_empty() {
        println!("(no worktrees)");
    } else {
        print!("{}", render_table(&plan));
        print_recommendations(&plan);
    }

    if !apply {
        if !plan.is_empty() && !ctx.is_json() {
            println!();
            println!("Dry run — nothing changed. Re-run with --yes to apply.");
        }
        return Ok(Outcome::ok());
    }

    let outcome = execute(ctx, &plan);
    if !ctx.is_json() {
        println!();
        println!("🌳 Pruning worktree metadata...");
        if let Err(e) = ctx.git.worktree_prune() {
            eprintln!("warning: {e}");
        }
        let counts = plan.counts();
        println!();
        println!(
            "✅ Done — removed {} worktree(s), deleted {} branch(es), {} kept",
            counts.to_remove, counts.to_delete, counts.kept
        );
    }
    Ok(outcome)
}

/// Applying requires an explicit opt-in, and the default posture is a dry run
/// whenever a human is not there to watch.
fn should_apply(args: &CleanArgs, plan: &Plan) -> bool {
    if plan.destructive().count() == 0 {
        return false;
    }
    if args.dry_run {
        return false;
    }
    if args.yes {
        return true;
    }
    // Interactive: yes. Piped or scripted: plan only.
    std::io::stdout().is_terminal()
}

fn attach_github(
    ctx: &Context,
    snap: &mut gw_core::RepoSnapshot,
) -> Result<HashSet<String>, Error> {
    let remote = ctx.git.remote_url()?.unwrap_or_default();
    let Some(slug) = repo_slug(&remote) else {
        eprintln!("warning: origin is not a GitHub remote; skipping merged-PR lookup");
        return Ok(HashSet::new());
    };
    let candidates = snapshot::pr_candidates(snap);
    if candidates.is_empty() {
        return Ok(HashSet::new());
    }
    println!(
        "🔄 Checking GitHub for {} merged PR(s)...",
        candidates.len()
    );

    let backend: Box<dyn GithubBackend> = if std::env::var_os("GW_SKIP_GITHUB").is_some() {
        Box::new(NoGithub)
    } else {
        Box::new(GhCli)
    };
    // A lookup failure must not silently reduce cleaning to stale-only; say so
    // and continue with what we can still prove.
    match backend.merged_prs(&slug, &candidates) {
        Ok(merged) => Ok(merged),
        Err(e) => {
            eprintln!("warning: {e}");
            eprintln!("         continuing without merged-PR information");
            Ok(HashSet::new())
        }
    }
}

fn attach_jira(ctx: &Context, snap: &mut gw_core::RepoSnapshot) {
    let Some(jira) = JiraRest::from_settings(&ctx.settings) else {
        return;
    };
    let keys = snapshot::ticket_keys(snap, &ctx.settings.jira_projects);
    if keys.is_empty() {
        return;
    }
    let backend: Box<dyn JiraBackend> = Box::new(jira);
    match backend.lookup(&keys) {
        Ok(info) => snapshot::attach_tickets(snap, &info, &ctx.settings.jira_projects),
        Err(e) => eprintln!("warning: {e}"),
    }
}

fn print_recommendations(plan: &Plan) {
    let recommended: Vec<_> = plan
        .entries
        .iter()
        .filter(|(_, d)| d.is_recommendation())
        .collect();
    if recommended.is_empty() {
        return;
    }
    println!();
    println!(
        "🎫 {} worktree(s) can be removed with --closed:",
        recommended.len()
    );
    for (target, decision) in recommended {
        let name = match target {
            Target::Worktree { branch, path } => branch
                .as_ref()
                .map_or_else(|| path.display().to_string(), |b| b.as_str().to_string()),
            Target::Branch { name } => name.as_str().to_string(),
        };
        println!(
            "   {name} — {}",
            gw_core::render::human::describe_reason(decision.reason())
        );
    }
}

/// Apply a plan. Never re-derives a decision.
fn execute(ctx: &Context, plan: &Plan) -> Outcome {
    let mut succeeded = 0;
    let mut failed = 0;
    let mut deleted: Vec<(String, String)> = Vec::new();

    for (target, decision) in &plan.entries {
        match (target, decision) {
            (Target::Worktree { path, branch }, Decision::Remove { .. }) => {
                // Never hand git a path outside the managed directory, even if
                // a snapshot somehow contained one.
                if !snapshot::is_managed(&ctx.layout, path) && !Layout::is_harness_worktree(path) {
                    eprintln!("refusing to remove unmanaged path: {}", path.display());
                    failed += 1;
                    continue;
                }
                match ctx.git.worktree_remove(path) {
                    Ok(()) => {
                        succeeded += 1;
                        println!(
                            "🗑️  removed {} ({})",
                            branch.as_ref().map_or_else(
                                || path.display().to_string(),
                                |b| b.as_str().to_string()
                            ),
                            gw_core::render::human::describe_reason(decision.reason())
                        );
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!("❌ could not remove {}: {e}", path.display());
                    }
                }
            }
            (Target::Branch { name }, Decision::Delete { tip, .. }) => {
                match ctx.git.branch_delete(name) {
                    Ok(()) => {
                        succeeded += 1;
                        deleted.push((name.as_str().to_string(), tip.as_str().to_string()));
                        println!("🗑️  deleted branch {} at {}", name.as_str(), tip.short(11));
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!("❌ could not delete {}: {e}", name.as_str());
                    }
                }
            }
            _ => {}
        }
    }

    if !deleted.is_empty() {
        println!();
        println!("Deleted branches can be restored with:");
        for (name, sha) in &deleted {
            println!("   {}", restore_hint(name, sha));
        }
    }

    if failed > 0 {
        return if succeeded > 0 {
            Outcome::partial()
        } else {
            Outcome { code: 1 }
        };
    }
    Outcome::ok()
}
