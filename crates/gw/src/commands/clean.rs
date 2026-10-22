use std::io::IsTerminal;

use gw_core::render::table::render_table;
use gw_core::{
    classify::classify_clean, decision::Decision, decision::Target, render::human::restore_hint,
    Layout, Plan, Policy,
};

use crate::cli::CleanArgs;
use crate::config::now_epoch;
use crate::context::Context;
use crate::error::Error;
use crate::git::GitBackend;
use crate::output::Outcome;
use crate::providers::Limits;

use super::snapshot;

/// The stale window, highest precedence first: the flag or `GW_STALE_DAYS`,
/// then `[clean] stale_after` from `.gw.toml`, then the built-in default.
///
/// Precedence lives here, at the adapter boundary, so `gw-core` keeps taking a
/// plain number of seconds and never learns where it came from.
fn resolve_stale_after_secs(ctx: &Context, args: &CleanArgs) -> i64 {
    if let Some(days) = args.stale_days {
        return days.saturating_mul(86_400);
    }
    if let Some(seconds) = ctx.loaded.clean.stale_after_secs {
        return seconds;
    }
    Policy::default().stale_after_secs
}

pub fn run(ctx: &Context, args: CleanArgs) -> Result<Outcome, Error> {
    let policy = Policy {
        stale_after_secs: resolve_stale_after_secs(ctx, &args),
        clean_closed_tickets: args.closed,
        delete_branches: args.delete_branches,
    };

    let now = now_epoch();
    // In JSON mode stdout carries exactly one JSON document. Every progress
    // line goes to stderr so a consumer can parse stdout without stripping.
    if !ctx.is_json() {
        println!(
            "{}Scanning worktrees in {}",
            ctx.decor("🔍 "),
            ctx.layout.root.display()
        );
    }

    // Refresh remote refs so "unpublished" is judged against current origin.
    if let Err(e) = ctx.git.fetch_prune() {
        eprintln!("warning: could not fetch, judging against cached refs ({e})");
    }

    let mut snap = snapshot::load(ctx, now)?;

    let registry = ctx.registry();
    let limits = Limits::default();

    // Merged changes. Each provider decides for itself whether it understands
    // this remote; a provider that does not reports why, rather than making the
    // worktree silently look unmerged.
    let queries = snapshot::change_queries(&snap, &ctx.convention);
    if !queries.is_empty() {
        let branches: std::collections::BTreeSet<&str> =
            queries.iter().map(|q| q.branch.as_str()).collect();
        let progress = format!(
            "{}Checking {} change(s) across {} worktree(s)...",
            ctx.decor("🔄 "),
            queries.len(),
            branches.len()
        );
        if ctx.is_json() {
            eprintln!("{progress}");
        } else {
            println!("{progress}");
        }
        let repo = snapshot::repo_ref_for(ctx);
        let observed = snapshot::observe_changes(registry, &repo, &queries, limits);
        snapshot::attach_changes(&mut snap, &ctx.convention, &observed);
    }

    // Tickets.
    let refs = snapshot::ticket_refs(&snap, &ctx.convention);
    if !refs.is_empty() {
        let observations = snapshot::observe_tickets(registry, &refs, limits);
        snapshot::attach_tickets(&mut snap, &ctx.convention, &observations);
    }

    let plan = classify_clean(&snap, &policy);
    let apply = should_apply(&args, &plan, ctx.is_json());

    if !ctx.is_json() {
        if plan.is_empty() {
            println!("(no worktrees)");
        } else {
            print!("{}", render_table(&plan));
            print_recommendations(ctx, &plan);
        }
    }

    if !apply {
        if ctx.is_json() {
            println!("{}", gw_core::render::json::to_json_string(&plan, false));
        }
        if announces_dry_run(&args, &plan, ctx.is_json()) {
            println!();
            println!("Dry run — nothing changed. Re-run with --yes to apply.");
        }
        return Ok(Outcome::ok());
    }

    let outcome = execute(ctx, &plan);

    if ctx.is_json() {
        // Printed after execution so `applied: true` describes work that has
        // already happened. `execute` writes its own lines to stderr in JSON
        // mode, so stdout stays a single document.
        println!("{}", gw_core::render::json::to_json_string(&plan, true));
    } else {
        let counts = plan.counts();
        println!();
        println!(
            "{}Done — removed {} worktree(s), deleted {} branch(es), pruned {} metadata, {} kept",
            ctx.decor("✅ "),
            counts.to_remove,
            counts.to_delete,
            counts.to_prune,
            counts.kept
        );
    }
    Ok(outcome)
}

/// Whether to print the "Dry run — nothing changed" notice.
///
/// It appears only when the plan is non-empty, the output is human-facing, and
/// `--yes` was not supplied. A `--yes` run whose plan had no work to do is not
/// a dry run and must not claim to be one.
fn announces_dry_run(args: &CleanArgs, plan: &Plan, json: bool) -> bool {
    !plan.is_empty() && !json && !args.yes
}

/// Applying requires planned work and an explicit opt-in, and the default
/// posture is a dry run whenever a human is not there to watch.
///
/// The gate is *planned work*, not destructiveness: a prune-only plan has
/// real, non-destructive work and must be applicable. Plans with nothing to
/// remove, delete, or prune never run.
///
/// A caller that asked for JSON is treated as a script even when stdout is a
/// terminal: an agent harness may allocate a pty, and `--json` is a request
/// for machine output, not a confirmation. Only `--yes` applies there.
fn should_apply(args: &CleanArgs, plan: &Plan, json: bool) -> bool {
    let counts = plan.counts();
    if counts.to_remove + counts.to_delete + counts.to_prune == 0 {
        return false;
    }
    if args.dry_run {
        return false;
    }
    if args.yes {
        return true;
    }
    if json {
        return false;
    }
    // Interactive: yes. Piped or scripted: plan only.
    std::io::stdout().is_terminal()
}

fn print_recommendations(ctx: &Context, plan: &Plan) {
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
        "{}{} worktree(s) can be removed with --closed:",
        ctx.decor("🎫 "),
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
    let mut pruned: Vec<String> = Vec::new();
    // In JSON mode stdout is reserved for the single plan document, so the
    // per-action lines go to stderr like every other progress line.
    let narrate = !ctx.is_json();

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
                        if narrate {
                            println!(
                                "{}removed {} ({})",
                                ctx.decor("🗑️  "),
                                branch.as_ref().map_or_else(
                                    || path.display().to_string(),
                                    |b| b.as_str().to_string()
                                ),
                                gw_core::render::human::describe_reason(decision.reason())
                            );
                        }
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!(
                            "{}could not remove {}: {e}",
                            ctx.decor("❌ "),
                            path.display()
                        );
                    }
                }
            }
            (Target::Branch { name }, Decision::Delete { tip, .. }) => {
                match ctx.git.branch_delete(name) {
                    Ok(()) => {
                        succeeded += 1;
                        deleted.push((name.as_str().to_string(), tip.as_str().to_string()));
                        if narrate {
                            println!(
                                "{}deleted branch {} at {}",
                                ctx.decor("🗑️  "),
                                name.as_str(),
                                tip.short(11)
                            );
                        }
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!(
                            "{}could not delete {}: {e}",
                            ctx.decor("❌ "),
                            name.as_str()
                        );
                    }
                }
            }
            (Target::Worktree { path, branch }, Decision::Prune { .. }) => {
                // Recorded, not executed per target: `git worktree prune` is
                // repository-wide (see below).
                pruned.push(
                    branch
                        .as_ref()
                        .map_or_else(|| path.display().to_string(), |b| b.as_str().to_string()),
                );
            }
            _ => {}
        }
    }

    if !pruned.is_empty() {
        // `git worktree prune` has no per-target form. It drops metadata for
        // every prunable entry in the repository, including one the plan
        // classified Keep or Skip. That is metadata-only: no ref and no
        // directory is deleted. A failure is a warning and does not change the
        // exit code, because the next run retries it.
        match ctx.git.worktree_prune() {
            Ok(()) => {
                if narrate {
                    for name in &pruned {
                        println!("{}pruned metadata for {name}", ctx.decor("🧹 "));
                    }
                }
            }
            Err(e) => eprintln!("warning: {e}"),
        }
    }

    if !deleted.is_empty() {
        if narrate {
            println!();
            println!("Deleted branches can be restored with:");
        }
        for (name, sha) in &deleted {
            // The restore hint is the payload for a deleted branch; it must be
            // available to scripts too, so it goes to stderr in JSON mode.
            let hint = restore_hint(name, sha);
            if narrate {
                println!("   {hint}");
            } else {
                eprintln!("{hint}");
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use gw_core::decision::{Decision, Reason};
    use gw_core::model::{BranchName, Sha};
    use gw_core::Plan;

    pub(super) fn args(yes: bool, dry_run: bool) -> CleanArgs {
        CleanArgs {
            dry_run,
            yes,
            closed: false,
            delete_branches: false,
            stale_days: Some(7),
        }
    }

    /// A plan with one destructive entry: enough for `should_apply` to be
    /// willing to act.
    pub(super) fn destructive_plan() -> Plan {
        Plan {
            entries: vec![(
                Target::Branch {
                    name: BranchName::parse("PROJ-1").unwrap(),
                },
                Decision::Delete {
                    reason: Reason::Stale { age_secs: 100 },
                    tip: Sha::parse("9df03e38ae0").unwrap(),
                },
            )],
        }
    }

    pub(super) fn non_destructive_plan() -> Plan {
        Plan {
            entries: vec![(
                Target::Branch {
                    name: BranchName::parse("PROJ-2").unwrap(),
                },
                Decision::Keep {
                    reason: Reason::RecentlyActive { age_secs: 1 },
                },
            )],
        }
    }

    /// A plan whose only work is non-destructive: prune metadata.
    pub(super) fn prune_only_plan() -> Plan {
        Plan {
            entries: vec![(
                Target::Worktree {
                    path: std::path::PathBuf::from("/repo/worktrees/feature/x"),
                    branch: Some(BranchName::parse("feature/x").unwrap()),
                },
                Decision::Prune {
                    reason: Reason::Prunable,
                },
            )],
        }
    }

    #[test]
    fn a_plan_with_nothing_destructive_never_applies() {
        assert!(!should_apply(
            &args(true, false),
            &non_destructive_plan(),
            false
        ));
        assert!(!should_apply(
            &args(true, false),
            &non_destructive_plan(),
            true
        ));
    }

    #[test]
    fn yes_applies() {
        assert!(should_apply(&args(true, false), &destructive_plan(), false));
        assert!(should_apply(&args(true, false), &destructive_plan(), true));
    }

    #[test]
    fn dry_run_wins_over_yes() {
        assert!(!should_apply(&args(true, true), &destructive_plan(), false));
    }

    /// The regression this guards: a harness that allocates a pty makes
    /// `stdout.is_terminal()` true. `--json` must still mean "plan only"
    /// without `--yes`, or an agent asking for a report destroys worktrees.
    #[test]
    fn json_never_applies_without_yes() {
        assert!(!should_apply(
            &args(false, false),
            &destructive_plan(),
            true
        ));
    }

    #[test]
    fn a_prune_only_plan_is_admitted_by_yes() {
        assert!(should_apply(&args(true, false), &prune_only_plan(), false));
        assert!(should_apply(&args(true, false), &prune_only_plan(), true));
        // A dry run refuses even a non-destructive plan.
        assert!(!should_apply(&args(true, true), &prune_only_plan(), false));
        // JSON without --yes stays a plan.
        assert!(!should_apply(&args(false, false), &prune_only_plan(), true));
        // Without --yes the terminal decides, exactly as for destructive work.
        assert_eq!(
            should_apply(&args(false, false), &prune_only_plan(), false),
            std::io::stdout().is_terminal()
        );
    }

    #[test]
    fn the_dry_run_notice_requires_no_yes_a_non_empty_plan_and_human_output() {
        assert!(announces_dry_run(
            &args(false, false),
            &prune_only_plan(),
            false
        ));
        assert!(!announces_dry_run(
            &args(true, false),
            &prune_only_plan(),
            false
        ));
        assert!(!announces_dry_run(
            &args(true, true),
            &prune_only_plan(),
            false
        ));
        assert!(!announces_dry_run(
            &args(false, false),
            &prune_only_plan(),
            true
        ));
        // A non-empty Keep-only plan still describes a plan, so the notice is
        // accurate. Only `--yes` suppresses it.
        assert!(announces_dry_run(
            &args(false, false),
            &non_destructive_plan(),
            false
        ));
        assert!(!announces_dry_run(&args(false, false), &Plan::new(), false));
    }
}

#[cfg(test)]
mod render_tests {
    use super::tests::{args, destructive_plan, non_destructive_plan};
    use super::*;
    use crate::context::Context;
    use gw_core::decision::Reason;
    use gw_core::model::BranchName;

    fn render_ctx() -> Context {
        Context::for_test(
            Layout::new(std::path::PathBuf::from("/repo")),
            Default::default(),
        )
    }

    fn worktree(branch: Option<&str>) -> Target {
        Target::Worktree {
            path: std::path::PathBuf::from("/repo/worktrees/PROJ-1"),
            branch: branch.map(|b| BranchName::parse(b).unwrap()),
        }
    }

    fn branch(name: &str) -> Target {
        Target::Branch {
            name: BranchName::parse(name).unwrap(),
        }
    }

    #[test]
    fn recommendations_are_printed_for_both_target_shapes() {
        let mut plan = Plan::new();
        plan.push(
            worktree(Some("PROJ-1")),
            Decision::Recommend {
                reason: Reason::TicketClosed {
                    resolution: "Done".into(),
                },
            },
        );
        plan.push(
            worktree(None),
            Decision::Recommend {
                reason: Reason::TicketClosed {
                    resolution: "Done".into(),
                },
            },
        );
        plan.push(
            branch("PROJ-2"),
            Decision::Recommend {
                reason: Reason::TicketClosed {
                    resolution: "Fixed".into(),
                },
            },
        );
        print_recommendations(&render_ctx(), &plan);
    }

    #[test]
    fn a_plan_without_recommendations_prints_nothing() {
        let mut plan = Plan::new();
        plan.push(
            branch("PROJ-1"),
            Decision::Keep {
                reason: Reason::Protected,
            },
        );
        print_recommendations(&render_ctx(), &plan);
    }

    fn stdin_is_a_terminal() -> bool {
        use std::io::IsTerminal;
        std::io::stdout().is_terminal()
    }

    #[test]
    fn interactive_confirmation_depends_on_a_terminal() {
        // Under `cargo test` stdout is a pipe, so the interactive arm must not
        // apply; when run in a terminal it may. Either way the call is total.
        let apply = should_apply(&args(false, false), &destructive_plan(), false);
        assert_eq!(apply, stdin_is_a_terminal());
        // A dry run never applies, whatever the terminal says.
        assert!(!should_apply(
            &args(false, true),
            &destructive_plan(),
            false
        ));
    }

    #[test]
    fn a_non_destructive_plan_runs_nothing() {
        assert!(!should_apply(
            &args(false, false),
            &non_destructive_plan(),
            false
        ));
    }
}
