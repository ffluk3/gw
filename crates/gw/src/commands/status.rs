use gw_core::render::human::short_age;
use gw_core::render::json::display_branch;
use gw_core::{TicketStatus, WorktreeKind};

use crate::cli::StatusArgs;
use crate::config::{display_path, now_epoch};
use crate::context::Context;
use crate::error::Error;
use crate::jira::{JiraBackend, JiraRest, NoJira};
use crate::output::Outcome;

use super::snapshot;

pub fn run(ctx: &Context, args: StatusArgs) -> Result<Outcome, Error> {
    let now = now_epoch();
    let mut snap = snapshot::load(ctx, now)?;

    // JIRA summaries are best-effort: a failure degrades the table, it does
    // not fail the command.
    let jira: Box<dyn JiraBackend> = if args.no_jira {
        Box::new(NoJira)
    } else {
        match JiraRest::from_settings(&ctx.settings) {
            Some(j) => Box::new(j),
            None => Box::new(NoJira),
        }
    };

    let keys = snapshot::ticket_keys(&snap, &ctx.settings.jira_projects);
    let info = if keys.is_empty() {
        crate::jira::TicketInfo::default()
    } else {
        match jira.lookup(&keys) {
            Ok(info) => info,
            Err(e) => {
                eprintln!("warning: {e}");
                crate::jira::TicketInfo::default()
            }
        }
    };
    snapshot::attach_tickets(&mut snap, &info, &ctx.settings.jira_projects);

    if ctx.is_json() {
        print_json(&snap, &info, &ctx.settings.jira_projects);
        return Ok(Outcome::ok());
    }

    print_table(&snap, &info, &ctx.settings.jira_projects);
    Ok(Outcome::ok())
}

fn print_json(snap: &gw_core::RepoSnapshot, info: &crate::jira::TicketInfo, projects: &[String]) {
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
    }

    let rows: Vec<Row> = snap
        .worktrees
        .iter()
        .filter(|wt| wt.kind != WorktreeKind::Main)
        .map(|wt| {
            let ticket = wt
                .branch
                .as_ref()
                .and_then(|branch| branch.ticket_key_for(projects));
            Row {
                branch: display_branch(wt.branch.as_ref()),
                path: display_path(&wt.path, &snap.layout.root),
                age_secs: Some(wt.activity.age_secs),
                dirty: !wt.dirty.is_clean(),
                dirty_state: wt.dirty.as_str(),
                summary: ticket
                    .as_ref()
                    .and_then(|k| info.summary.get(k.as_str()).cloned()),
                ticket: ticket.as_ref().map(|k| k.as_str().to_string()),
                kind: wt.kind,
                ticket_state: wt.ticket.clone(),
            }
        })
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({ "worktrees": rows }))
            .unwrap_or_else(|_| "{}".into())
    );
}

fn print_table(snap: &gw_core::RepoSnapshot, info: &crate::jira::TicketInfo, projects: &[String]) {
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
            _ => match wt.ticket.closed_resolution() {
                Some(_) => "closed",
                None => "",
            },
        };
        let summary = wt
            .branch
            .as_ref()
            .and_then(|branch| branch.ticket_key_for(projects))
            .and_then(|k| info.summary.get(k.as_str()).cloned())
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
