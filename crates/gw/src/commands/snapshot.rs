use std::collections::BTreeSet;

use gw_core::{
    Activity, ActivitySource, BranchName, BranchSnapshot, Dirty, Layout, ReachableSet,
    RepoSnapshot, TicketStatus, WorktreeKind, WorktreeSnapshot,
};

use crate::context::Context;
use crate::error::Error;
use crate::git::GitBackend;

/// Gather every fact the classifier is allowed to see.
///
/// This is the only function that turns adapter output into domain state.
/// Anything it cannot observe becomes an explicit `Unknown` rather than a
/// silent default, so the classifier can fail closed.
pub fn load(ctx: &Context, now: i64) -> Result<RepoSnapshot, Error> {
    let entries = ctx.git.worktree_list()?;
    let mut worktrees = Vec::with_capacity(entries.len());

    for entry in &entries {
        let kind = entry.kind();
        let exists = entry.path.is_dir();

        let dirty = match kind {
            WorktreeKind::Main | WorktreeKind::Prunable => Dirty::Unknown,
            _ if !exists => Dirty::Dirty,
            _ => ctx.git.status(&entry.path)?,
        };

        let activity = if kind == WorktreeKind::Main {
            Activity::unknown()
        } else if exists {
            match ctx.git.last_commit_epoch(&entry.path)? {
                Some(epoch) => Activity {
                    age_secs: (now - epoch).max(0),
                    source: ActivitySource::CommitDate,
                },
                None => Activity::unknown(),
            }
        } else {
            Activity::unknown()
        };

        worktrees.push(WorktreeSnapshot {
            path: entry.path.clone(),
            branch: entry.branch.clone(),
            head: entry.head.clone(),
            kind,
            dirty,
            activity,
            ticket: TicketStatus::NotChecked,
            merged_pr: false,
        });
    }

    let branches = ctx.git.for_each_branch(now)?;
    let orphans: Vec<BranchSnapshot> = branches
        .iter()
        .filter(|b| b.worktree.is_none())
        .map(|b| BranchSnapshot {
            name: b.name.clone(),
            worktree: b.worktree.clone(),
            tip: b.tip.clone(),
            commit_age_secs: b.commit_age_secs,
        })
        .collect();

    let protected = protected_set(ctx, &entries)?;

    Ok(RepoSnapshot {
        layout: ctx.layout.clone(),
        default_branch: ctx.git.default_branch()?,
        head_branch: ctx.git.head_branch()?,
        protected,
        reachable: ReachableSet::new(ctx.git.rev_list_remotes()?),
        worktrees,
        orphans,
    })
}

/// Branches that must never be deleted.
fn protected_set(
    ctx: &Context,
    entries: &[crate::git::WorktreeEntry],
) -> Result<BTreeSet<BranchName>, Error> {
    let mut protected = BTreeSet::new();
    if let Some(default) = ctx.git.default_branch()? {
        protected.insert(default);
    }
    if let Some(head) = ctx.git.head_branch()? {
        protected.insert(head);
    }
    for entry in entries {
        if let Some(branch) = &entry.branch {
            protected.insert(branch.clone());
        }
    }
    for extra in std::env::var("GW_PROTECT")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.trim().is_empty())
    {
        if let Ok(name) = BranchName::parse(extra.trim()) {
            protected.insert(name);
        }
    }
    Ok(protected)
}

/// Attach GitHub merge state to the snapshot.
pub fn attach_merged(snapshot: &mut RepoSnapshot, merged: &std::collections::HashSet<String>) {
    for wt in &mut snapshot.worktrees {
        if let Some(branch) = &wt.branch {
            wt.merged_pr = merged.contains(branch.as_str());
        }
    }
}

/// Attach JIRA ticket state to the snapshot.
///
/// `projects` restricts which branch prefixes count as tickets; an empty slice
/// accepts any JIRA-shaped key.
pub fn attach_tickets(
    snapshot: &mut RepoSnapshot,
    info: &crate::jira::TicketInfo,
    projects: &[String],
) {
    for wt in &mut snapshot.worktrees {
        let Some(branch) = &wt.branch else { continue };
        let Some(key) = branch.ticket_key_for(projects) else {
            continue;
        };
        if let Some(status) = info.status.get(key.as_str()) {
            wt.ticket = status.clone();
        }
    }
}

/// Branch names eligible for a merged-PR lookup.
///
/// Only branches whose worktree directory is named after them are queried, to
/// preserve the shell implementation's guard against mismatched checkouts.
pub fn pr_candidates(snapshot: &RepoSnapshot) -> Vec<BranchName> {
    snapshot
        .worktrees
        .iter()
        .filter(|wt| matches!(wt.kind, WorktreeKind::Linked | WorktreeKind::Prunable))
        .filter_map(|wt| {
            let branch = wt.branch.as_ref()?;
            let folder = wt.path.file_name()?.to_str()?;
            if folder != branch.as_str() {
                return None;
            }
            Some(branch.clone())
        })
        .collect()
}

/// Total ticket keys referenced by the snapshot.
pub fn ticket_keys(snapshot: &RepoSnapshot, projects: &[String]) -> Vec<gw_core::TicketKey> {
    let mut keys: Vec<_> = snapshot
        .worktrees
        .iter()
        .filter_map(|wt| {
            wt.branch
                .as_ref()
                .and_then(|branch| branch.ticket_key_for(projects))
        })
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// Whether `path` is inside the configured worktrees directory.
///
/// Both sides are canonicalized first: on macOS a temporary directory is
/// `/var/...` in one API and `/private/var/...` in another, and git reports the
/// resolved form while configuration often carries the symlinked one. A raw
/// `starts_with` comparison silently fails in that case, which would make every
/// removal look unmanaged.
pub fn is_managed(layout: &Layout, path: &std::path::Path) -> bool {
    let managed =
        std::fs::canonicalize(&layout.worktrees).unwrap_or_else(|_| layout.worktrees.clone());
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    target.starts_with(&managed)
}
