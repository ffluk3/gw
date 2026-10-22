use std::collections::BTreeSet;

use gw_core::{
    Activity, ActivitySource, BranchName, BranchSnapshot, ChangeObservation, ChangeQuery,
    Convention, Dirty, Layout, ProviderId, ReachableSet, RepoSnapshot, Strength, TicketId,
    TicketObservation, TicketRef, TicketStatus, WorktreeKind, WorktreeSnapshot,
};

use crate::context::Context;
use crate::error::Error;
use crate::git::GitBackend;
use crate::providers::{Limits, Registry};

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
            tickets: Vec::new(),
            changes: Vec::new(),
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
    // `[clean] protect` from the files, then `GW_PROTECT`. Both are additive:
    // protection only ever grows, so a name that one source already listed is
    // still protected and no source can remove another's protection.
    for extra in ctx
        .loaded
        .clean
        .protect
        .iter()
        .map(String::as_str)
        .chain(
            std::env::var("GW_PROTECT")
                .unwrap_or_default()
                .split(',')
                .map(str::trim),
        )
        .filter(|s| !s.is_empty())
    {
        if let Ok(name) = BranchName::parse(extra) {
            protected.insert(name);
        }
    }
    Ok(protected)
}

/// Every ticket reference the convention derives from the snapshot's branches.
///
/// Deduplicated by `(provider, id)` because two branches can name one ticket.
#[must_use]
pub fn ticket_refs(snapshot: &RepoSnapshot, convention: &Convention) -> Vec<TicketRef> {
    let mut seen: BTreeSet<(ProviderId, TicketId)> = BTreeSet::new();
    let mut refs = Vec::new();
    for wt in &snapshot.worktrees {
        let Some(branch) = &wt.branch else { continue };
        for ticket in convention.resolve(branch.as_str()).tickets {
            if seen.insert((ticket.provider.clone(), ticket.id.clone())) {
                refs.push(ticket);
            }
        }
    }
    refs.sort_by(|a, b| a.provider.cmp(&b.provider).then_with(|| a.id.cmp(&b.id)));
    refs
}

/// Every change query the convention derives from the snapshot's worktrees.
///
/// Only linked and prunable worktrees are queried. The bare entry has no
/// worktree to remove, and asking a host about it would be network work that no
/// decision can use.
#[must_use]
pub fn change_queries(snapshot: &RepoSnapshot, convention: &Convention) -> Vec<ChangeQuery> {
    let mut queries = Vec::new();
    for wt in &snapshot.worktrees {
        if !matches!(wt.kind, WorktreeKind::Linked | WorktreeKind::Prunable) {
            continue;
        }
        let Some(branch) = &wt.branch else { continue };
        queries.extend(convention.resolve(branch.as_str()).changes);
    }
    queries
}

/// Attach ticket observations to the worktrees whose convention referenced them.
///
/// Every reference gets an observation. When no active provider answered, the
/// observation is `NotChecked`: the branch *does* name that ticket, which is
/// what the display and JSON `ticket` field report, but no state was observed.
/// That is neither `Open` nor `Unknown`, so it changes no decision.
pub fn attach_tickets(
    snapshot: &mut RepoSnapshot,
    convention: &Convention,
    observations: &[TicketObservation],
) {
    for wt in &mut snapshot.worktrees {
        let Some(branch) = &wt.branch else { continue };
        for reference in convention.resolve(branch.as_str()).tickets {
            let matched = observations
                .iter()
                .find(|o| o.provider == reference.provider && o.id == reference.id);
            wt.tickets.push(match matched {
                // Strength comes from the convention, not the provider: the
                // provider has no idea how the branch was matched.
                Some(observation) => TicketObservation {
                    strength: reference.strength,
                    ..observation.clone()
                },
                None => TicketObservation {
                    provider: reference.provider.clone(),
                    id: reference.id.clone(),
                    strength: reference.strength,
                    status: TicketStatus::NotChecked,
                    summary: None,
                },
            });
        }
    }
}

/// Attach change observations to the worktrees whose queries produced them.
pub fn attach_changes(
    snapshot: &mut RepoSnapshot,
    convention: &Convention,
    observations: &[(ProviderId, Vec<ChangeObservation>)],
) {
    for wt in &mut snapshot.worktrees {
        let Some(branch) = &wt.branch else { continue };
        for query in convention.resolve(branch.as_str()).changes {
            let Some((_, answers)) = observations
                .iter()
                .find(|(provider, _)| provider == &query.provider)
            else {
                continue;
            };
            // Match on the branch as well as the selector: two worktrees can
            // share a provider and a selector, and the first answer must not
            // be applied to both.
            if let Some(answer) = answers
                .iter()
                .find(|a| a.selector == query.selector.as_str() && a.branch == query.branch)
            {
                let mut observation = answer.clone();
                observation.strength = query.strength;
                wt.changes.push(observation);
            }
        }
    }
}

/// How a change provider should identify the repository.
///
/// The slug is `owner/name` when the origin is a GitHub remote, which is what
/// the GitHub adapter needs. Every provider also gets the raw remote, so a
/// provider for a different host is not blocked by a parser it does not use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub remote: Option<String>,
    pub slug: Option<String>,
}

impl RepoRef {
    #[must_use]
    pub fn from_remote(remote: Option<String>) -> Self {
        let slug = remote.as_deref().and_then(crate::config::repo_slug);
        Self { remote, slug }
    }
}

/// The repository the snapshot describes, as a change provider sees it.
#[must_use]
pub fn repo_ref_for(ctx: &Context) -> RepoRef {
    RepoRef::from_remote(ctx.git.remote_url().ok().flatten())
}

/// Ticket lookups grouped by provider, in the order providers first appear.
#[must_use]
pub fn tickets_by_provider(refs: &[TicketRef]) -> Vec<(ProviderId, Vec<TicketId>)> {
    let mut grouped: Vec<(ProviderId, Vec<TicketId>)> = Vec::new();
    for reference in refs {
        match grouped.iter_mut().find(|(p, _)| p == &reference.provider) {
            Some((_, ids)) => ids.push(reference.id.clone()),
            None => grouped.push((reference.provider.clone(), vec![reference.id.clone()])),
        }
    }
    grouped
}

/// Change queries grouped by provider, in the order providers first appear.
#[must_use]
pub fn queries_by_provider(queries: &[ChangeQuery]) -> Vec<(ProviderId, Vec<ChangeQuery>)> {
    let mut grouped: Vec<(ProviderId, Vec<ChangeQuery>)> = Vec::new();
    for query in queries {
        match grouped.iter_mut().find(|(p, _)| p == &query.provider) {
            Some((_, list)) => list.push(query.clone()),
            None => grouped.push((query.provider.clone(), vec![query.clone()])),
        }
    }
    grouped
}

/// Run the ticket lookups a convention asks for.
///
/// Every failure becomes an `Unknown` observation for the affected provider,
/// never an error: a tracker that is down must not stop `gw` from reporting
/// worktrees, and must not look like "no tickets".
#[must_use]
pub fn observe_tickets(
    registry: &Registry,
    refs: &[TicketRef],
    limits: Limits,
) -> Vec<TicketObservation> {
    let mut observations = Vec::new();
    for (provider, ids) in tickets_by_provider(refs) {
        let Some(backend) = registry.ticket_provider(&provider) else {
            warn_inactive(registry, &provider);
            continue;
        };
        match backend.lookup(&ids, limits) {
            Ok(mut answered) => observations.append(&mut answered),
            Err(e) => {
                eprintln!("warning: {e}");
                eprintln!("         continuing without {provider} ticket information");
                observations.extend(ids.into_iter().map(|id| TicketObservation {
                    provider: provider.clone(),
                    id,
                    strength: Strength::Identity,
                    status: TicketStatus::Unknown {
                        detail: e.to_string(),
                    },
                    summary: None,
                }));
            }
        }
    }
    observations
}

/// Run the change lookups a convention asks for.
#[must_use]
pub fn observe_changes(
    registry: &Registry,
    repo: &RepoRef,
    queries: &[ChangeQuery],
    limits: Limits,
) -> Vec<(ProviderId, Vec<ChangeObservation>)> {
    let mut observed = Vec::new();
    for (provider, list) in queries_by_provider(queries) {
        let Some(backend) = registry.change_provider(&provider) else {
            warn_inactive(registry, &provider);
            continue;
        };
        match backend.lookup(repo, &list, limits) {
            Ok(answers) => observed.push((provider, answers)),
            Err(e) => {
                eprintln!("warning: {e}");
                eprintln!("         continuing without {provider} change information");
                observed.push((
                    provider.clone(),
                    list.iter()
                        .map(|q| ChangeObservation {
                            provider: provider.clone(),
                            strength: q.strength,
                            selector: q.selector.as_str().to_string(),
                            branch: q.branch.clone(),
                            state: gw_core::ChangeState::Unknown {
                                detail: e.to_string(),
                            },
                        })
                        .collect(),
                ));
            }
        }
    }
    observed
}

/// Say why a provider the convention named did not answer.
///
/// Silence would be indistinguishable from "asked, found nothing", and both
/// leave a worktree alone for very different reasons.
fn warn_inactive(registry: &Registry, provider: &ProviderId) {
    if let Some(reason) = registry.inactive_reason(provider) {
        eprintln!("warning: provider {provider} is inactive ({reason})");
    }
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

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use gw_core::{ChangeRule, ChangeSelector, ChangeState, Sha, TicketRule, TicketSelector};

    fn sha(raw: &str) -> Sha {
        Sha::parse(raw).unwrap()
    }

    fn observation(provider: &str, id: &str) -> TicketObservation {
        TicketObservation {
            provider: ProviderId::parse(provider).unwrap(),
            id: TicketId::parse(id).unwrap(),
            strength: Strength::Identity,
            status: TicketStatus::Open,
            summary: Some("from the provider".into()),
        }
    }

    fn snapshot(worktrees: Vec<WorktreeSnapshot>) -> RepoSnapshot {
        RepoSnapshot {
            layout: Layout::new(std::path::PathBuf::from("/repo")),
            default_branch: None,
            head_branch: None,
            protected: Default::default(),
            reachable: Default::default(),
            worktrees,
            orphans: Vec::new(),
        }
    }

    fn worktree(branch: Option<&str>, kind: WorktreeKind) -> WorktreeSnapshot {
        WorktreeSnapshot {
            path: std::path::PathBuf::from("/repo/worktrees/x"),
            branch: branch.map(|b| BranchName::parse(b).unwrap()),
            head: Some(sha("aaaa")),
            kind,
            dirty: Dirty::Clean,
            activity: Activity::unknown(),
            tickets: Vec::new(),
            changes: Vec::new(),
        }
    }

    fn nothing() -> OrgGuard {
        OrgGuard
    }

    struct OrgGuard;

    impl Drop for OrgGuard {
        fn drop(&mut self) {}
    }

    #[test]
    fn an_answered_ticket_keeps_the_conventions_strength() {
        let convention = Convention {
            ticket_rules: vec![TicketRule {
                provider: ProviderId::parse("jira").unwrap(),
                selector: TicketSelector::Embedded {
                    pattern: "[A-Z]+-[0-9]+".into(),
                    allow_projects: vec![],
                },
                strength: Strength::Mention,
            }],
            change_rules: vec![],
        };
        let mut snap = snapshot(vec![worktree(Some("PROJ-1"), WorktreeKind::Linked)]);
        let observations = vec![observation("jira", "PROJ-1")];

        attach_tickets(&mut snap, &convention, &observations);

        let ticket = &snap.worktrees[0].tickets[0];
        assert_eq!(
            ticket.strength,
            Strength::Mention,
            "provider strength leaked"
        );
        assert_eq!(ticket.summary.as_deref(), Some("from the provider"));
    }

    #[test]
    fn an_unanswered_ticket_is_not_checked() {
        let convention = Convention::built_in();
        let mut snap = snapshot(vec![worktree(Some("PROJ-1"), WorktreeKind::Linked)]);
        attach_tickets(&mut snap, &convention, &[]);
        let ticket = &snap.worktrees[0].tickets[0];
        assert_eq!(ticket.status, TicketStatus::NotChecked);
    }

    #[test]
    fn ticket_refs_are_sorted_and_deduplicated() {
        let convention = Convention::built_in();
        let snap = snapshot(vec![
            worktree(Some("PROJ-2"), WorktreeKind::Linked),
            worktree(Some("PROJ-1"), WorktreeKind::Linked),
            worktree(Some("PROJ-1"), WorktreeKind::Prunable),
            worktree(None, WorktreeKind::Linked),
        ]);
        let refs = ticket_refs(&snap, &convention);
        let ids: Vec<&str> = refs.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["PROJ-1", "PROJ-2"]);
    }

    #[test]
    fn change_queries_skip_unqueryable_kinds() {
        let convention = Convention::built_in();
        let snap = snapshot(vec![
            worktree(Some("main"), WorktreeKind::Main),
            worktree(Some("PROJ-1"), WorktreeKind::Harness),
            worktree(Some("PROJ-2"), WorktreeKind::Linked),
            worktree(None, WorktreeKind::Linked),
            worktree(Some("PROJ-3"), WorktreeKind::Prunable),
        ]);
        let queries = change_queries(&snap, &convention);
        let branches: Vec<&str> = queries.iter().map(|q| q.branch.as_str()).collect();
        assert_eq!(branches, vec!["PROJ-2", "PROJ-3"]);
    }

    #[test]
    fn attaching_changes_matches_provider_selector_and_branch() {
        let convention = Convention::built_in();
        let mut snap = snapshot(vec![worktree(Some("PROJ-1"), WorktreeKind::Linked)]);
        let github = ProviderId::parse("github").unwrap();
        let answer = ChangeObservation {
            provider: github.clone(),
            strength: Strength::Mention,
            selector: "head_branch".into(),
            branch: "PROJ-1".into(),
            state: ChangeState::Merged { reference: None },
        };

        // A provider that is not named by the convention is ignored.
        attach_changes(
            &mut snap,
            &convention,
            &[(ProviderId::parse("other").unwrap(), vec![answer.clone()])],
        );
        assert!(snap.worktrees[0].changes.is_empty());

        attach_changes(&mut snap, &convention, &[(github, vec![answer])]);
        assert_eq!(snap.worktrees[0].changes.len(), 1);
        // Strength always comes from the query.
        assert_eq!(snap.worktrees[0].changes[0].strength, Strength::Identity);
    }

    #[test]
    fn grouping_preserves_first_appearance_order() {
        let refs = vec![
            TicketRef {
                provider: ProviderId::parse("jira").unwrap(),
                id: TicketId::parse("PROJ-1").unwrap(),
                strength: Strength::Identity,
                selector: "embedded".into(),
            },
            TicketRef {
                provider: ProviderId::parse("linear").unwrap(),
                id: TicketId::parse("ENG-2").unwrap(),
                strength: Strength::Identity,
                selector: "embedded".into(),
            },
            TicketRef {
                provider: ProviderId::parse("jira").unwrap(),
                id: TicketId::parse("PROJ-3").unwrap(),
                strength: Strength::Identity,
                selector: "embedded".into(),
            },
        ];
        let grouped = tickets_by_provider(&refs);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].0.as_str(), "jira");
        assert_eq!(grouped[0].1.len(), 2);

        let queries: Vec<ChangeQuery> = convention_queries(&["PROJ-1", "PROJ-2", "ENG-9"]);
        let grouped = queries_by_provider(&queries);
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].1.len(), 3);
    }

    fn convention_queries(branches: &[&str]) -> Vec<ChangeQuery> {
        let convention = Convention::built_in();
        branches
            .iter()
            .flat_map(|branch| convention.resolve(branch).changes)
            .collect()
    }

    #[test]
    fn an_inactive_provider_is_reported_and_skipped() {
        let registry = Registry::build(&Default::default(), Vec::new());
        let refs = vec![TicketRef {
            provider: ProviderId::parse("jira").unwrap(),
            id: TicketId::parse("PROJ-1").unwrap(),
            strength: Strength::Identity,
            selector: "embedded".into(),
        }];
        // No JIRA adapter is configured, so this yields no observation.
        assert!(observe_tickets(&registry, &refs, Limits::default()).is_empty());

        let queries = convention_queries(&["PROJ-1"]);
        let skipped = Registry::build(
            &Default::default(),
            vec![ProviderId::parse("github").unwrap()],
        );
        let repo = RepoRef::from_remote(None);
        assert!(observe_changes(&skipped, &repo, &queries, Limits::default()).is_empty());
    }

    #[test]
    fn a_failing_provider_becomes_unknown_observations() {
        // Point a command provider at a program this process cannot run, so
        // the lookup fails at spawn time and the error path runs end to end.
        let loaded = crate::convention_file::Loaded {
            convention: Convention {
                ticket_rules: vec![TicketRule {
                    provider: ProviderId::parse("host").unwrap(),
                    selector: TicketSelector::Embedded {
                        pattern: "[A-Z]+-[0-9]+".into(),
                        allow_projects: vec![],
                    },
                    strength: Strength::Identity,
                }],
                change_rules: vec![ChangeRule {
                    provider: ProviderId::parse("host").unwrap(),
                    selector: ChangeSelector::HeadBranch,
                    strength: Strength::Identity,
                }],
            },
            providers: vec![crate::convention_file::ProviderConfig {
                id: ProviderId::parse("host").unwrap(),
                kind: "command".into(),
                base_url: None,
                done_status_category: None,
                command: Some(vec!["/definitely/not-a-real-binary".into()]),
            }],
            ..Default::default()
        };
        let registry = Registry::build(&loaded, Vec::new());
        let refs = vec![TicketRef {
            provider: ProviderId::parse("host").unwrap(),
            id: TicketId::parse("PROJ-1").unwrap(),
            strength: Strength::Identity,
            selector: "embedded".into(),
        }];
        let observations = observe_tickets(&registry, &refs, Limits::default());
        assert_eq!(observations.len(), 1);
        assert!(matches!(
            observations[0].status,
            TicketStatus::Unknown { .. }
        ));

        let queries = loaded.convention.resolve("PROJ-1").changes;
        let repo = RepoRef::from_remote(None);
        let observed = observe_changes(&registry, &repo, &queries, Limits::default());
        assert_eq!(observed.len(), 1);
        assert!(matches!(
            observed[0].1[0].state,
            ChangeState::Unknown { .. }
        ));
    }

    #[test]
    fn a_repo_ref_derives_a_slug_only_for_github() {
        assert_eq!(
            RepoRef::from_remote(Some("git@github.com:acme/widgets.git".into()))
                .slug
                .as_deref(),
            Some("acme/widgets")
        );
        assert!(RepoRef::from_remote(Some("https://gitlab.com/a/b".into()))
            .slug
            .is_none());
        let _ = nothing();
    }

    #[test]
    fn a_managed_path_is_inside_the_worktrees_directory() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path().to_path_buf());
        std::fs::create_dir_all(&layout.worktrees).unwrap();
        let inside = layout.worktrees.join("PROJ-1");
        std::fs::create_dir_all(&inside).unwrap();
        assert!(is_managed(&layout, &inside));
        assert!(!is_managed(&layout, &dir.path().join("elsewhere")));
    }
}
