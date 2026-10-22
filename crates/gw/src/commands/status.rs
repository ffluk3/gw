use gw_core::render::human::short_age;
use gw_core::render::json::display_branch;
use gw_core::{TicketObservation, TicketStatus, WorktreeKind};

use crate::cli::StatusArgs;
use crate::config::{display_path, now_epoch};
use crate::context::Context;
use crate::error::Error;
use crate::output::Outcome;
use crate::providers::Limits;

use super::snapshot;

pub fn run(ctx: &Context, _args: StatusArgs) -> Result<Outcome, Error> {
    let now = now_epoch();
    let mut snap = snapshot::load(ctx, now)?;

    // Ticket summaries are best-effort: a failure degrades the table, it does
    // not fail the command. Use `--skip-provider` to avoid the lookup entirely.
    let refs = snapshot::ticket_refs(&snap, &ctx.convention);
    let observations = snapshot::observe_tickets(ctx.registry(), &refs, Limits::default());
    snapshot::attach_tickets(&mut snap, &ctx.convention, &observations);

    if ctx.is_json() {
        print_json(&snap);
        return Ok(Outcome::ok());
    }

    print_table(&snap);
    Ok(Outcome::ok())
}

/// The summary to show for a worktree: the primary ticket's, when one exists.
fn summary_of(ticket: Option<&TicketObservation>) -> Option<String> {
    ticket.and_then(|t| t.summary.clone())
}

/// `Option::map` adapter so a `Option<&TicketObservation>` chain stays flat.
fn summary_maybe(ticket: &TicketObservation) -> Option<String> {
    ticket.summary.clone()
}

fn print_json(snap: &gw_core::RepoSnapshot) {
    #[derive(serde::Serialize)]
    struct Row {
        branch: String,
        path: String,
        age_secs: Option<i64>,
        dirty: bool,
        dirty_state: &'static str,
        ticket: Option<String>,
        summary: Option<String>,
        kind: WorktreeKind,
        ticket_state: TicketStatus,
        tickets: Vec<TicketObservation>,
    }

    let rows: Vec<Row> = snap
        .worktrees
        .iter()
        .filter(|wt| wt.kind != WorktreeKind::Main)
        .map(|wt| {
            let ticket = wt.primary_known_ticket().or_else(|| wt.primary_ticket());
            Row {
                branch: display_branch(wt.branch.as_ref()),
                path: display_path(&wt.path, &snap.layout.root),
                age_secs: Some(wt.activity.age_secs),
                dirty: !wt.dirty.is_clean(),
                dirty_state: wt.dirty.as_str(),
                summary: summary_of(ticket),
                ticket: ticket.map(|t| t.id.as_str().to_string()),
                kind: wt.kind,
                ticket_state: ticket.map_or(TicketStatus::NotChecked, |t| t.status.clone()),
                tickets: wt.tickets.clone(),
            }
        })
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({ "worktrees": rows }))
            .unwrap_or_else(|_| "{}".into())
    );
}

fn print_table(snap: &gw_core::RepoSnapshot) {
    let mut rows: Vec<[String; 5]> = Vec::new();
    rows.push([
        "BRANCH".into(),
        "LAST COMMIT".into(),
        "DIRTY".into(),
        "STATE".into(),
        "SUMMARY".into(),
    ]);

    let mut dirty_count = 0;
    let mut shown = 0;

    for wt in &snap.worktrees {
        if wt.kind == WorktreeKind::Main {
            continue;
        }
        shown += 1;
        let branch = display_branch(wt.branch.as_ref());
        let age = if wt.activity.source == gw_core::ActivitySource::Unknown {
            "—".to_string()
        } else {
            short_age(wt.activity.age_secs)
        };
        let dirty = if wt.dirty.is_clean() {
            " ".to_string()
        } else {
            dirty_count += 1;
            "●".to_string()
        };
        let state = match wt.kind {
            WorktreeKind::Harness => "harness",
            WorktreeKind::Prunable => "prunable",
            _ => match wt.closed_ticket_resolution() {
                Some(_) => "closed",
                None => "",
            },
        };
        let summary = wt
            .primary_known_ticket()
            .or_else(|| wt.primary_ticket())
            .and_then(summary_maybe)
            .unwrap_or_default();

        rows.push([branch, age, dirty, state.to_string(), summary]);
    }

    if shown == 0 {
        println!("(no worktrees)");
        return;
    }

    println!("{}", crate::output::layout_rows(&rows));

    if dirty_count > 0 {
        println!();
        println!("● = uncommitted changes");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_label_prefers_the_observation() {
        let observation = TicketObservation {
            provider: gw_core::ProviderId::parse("jira").unwrap(),
            id: gw_core::TicketId::parse("PROJ-1").unwrap(),
            strength: gw_core::Strength::Identity,
            status: TicketStatus::Open,
            summary: Some("Add rate limiting".into()),
        };
        assert_eq!(
            summary_of(Some(&observation)).as_deref(),
            Some("Add rate limiting")
        );
        assert!(summary_of(None).is_none());
    }
}

#[cfg(test)]
mod snapshot_fixture {
    use gw_core::{
        Activity, ActivitySource, ChangeObservation, ChangeState, Dirty, Layout, ProviderId,
        ReachableSet, RepoSnapshot, Strength, TicketId, TicketObservation, TicketStatus,
        WorktreeKind, WorktreeSnapshot,
    };

    use super::print_table;

    fn ticket(
        status: TicketStatus,
        strength: Strength,
        summary: Option<&str>,
    ) -> TicketObservation {
        TicketObservation {
            provider: ProviderId::parse("jira").unwrap(),
            id: TicketId::parse("PROJ-1").unwrap(),
            strength,
            status,
            summary: summary.map(str::to_string),
        }
    }

    fn worktree(path: &str, kind: WorktreeKind) -> WorktreeSnapshot {
        WorktreeSnapshot {
            path: std::path::PathBuf::from(path),
            branch: Some(gw_core::BranchName::parse("PROJ-1").unwrap()),
            head: Some(gw_core::Sha::parse("aaaa").unwrap()),
            kind,
            dirty: Dirty::Clean,
            activity: Activity {
                age_secs: 120,
                source: ActivitySource::CommitDate,
            },
            tickets: Vec::new(),
            changes: Vec::new(),
        }
    }

    fn snapshot(worktrees: Vec<WorktreeSnapshot>) -> RepoSnapshot {
        RepoSnapshot {
            layout: Layout::new(std::path::PathBuf::from("/repo")),
            default_branch: None,
            head_branch: None,
            protected: Default::default(),
            reachable: ReachableSet::default(),
            worktrees,
            orphans: Vec::new(),
        }
    }

    // The table and JSON printers write to stdout, which the test harness
    // captures. These tests assert the branches are exercised without a panic;
    // the JSON shape is covered end to end in `tests/cli.rs`.

    #[test]
    fn a_table_with_only_the_bare_entry_says_there_are_no_worktrees() {
        print_table(&snapshot(vec![worktree("/repo/.bare", WorktreeKind::Main)]));
    }

    #[test]
    fn a_table_reports_harness_prunable_closed_dirty_and_unknown_rows() {
        let mut harness = worktree("/repo/wt/harness", WorktreeKind::Harness);
        harness.activity.source = ActivitySource::Unknown;
        harness.dirty = Dirty::Dirty;
        harness.branch = None;

        let mut prunable = worktree("/repo/wt/prunable", WorktreeKind::Prunable);
        prunable.dirty = Dirty::Unknown;
        prunable.tickets = vec![ticket(
            TicketStatus::Closed {
                resolution: Some("Done".into()),
            },
            Strength::Identity,
            Some("Ship it"),
        )];

        let mut linked = worktree("/repo/wt/linked", WorktreeKind::Linked);
        linked.tickets = vec![ticket(TicketStatus::NotChecked, Strength::Identity, None)];
        linked.changes = vec![ChangeObservation {
            provider: ProviderId::parse("github").unwrap(),
            strength: Strength::Identity,
            selector: "head_branch".into(),
            branch: "PROJ-1".into(),
            state: ChangeState::Merged { reference: None },
        }];

        print_table(&snapshot(vec![
            worktree("/repo/.bare", WorktreeKind::Main),
            harness,
            prunable,
            linked,
        ]));
    }
}
