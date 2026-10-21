use std::path::PathBuf;

use gw_core::{BranchName, Sha};

use super::backend::WorktreeEntry;

/// Expand a leading `~` to the home directory.
///
/// Git abbreviates paths under `$HOME` to `~/...` in some configurations, and
/// a raw `PathBuf::from("~/x")` is a *relative* path that silently points
/// nowhere. Every consumer downstream assumes absolute paths.
fn expand_home(value: &str) -> PathBuf {
    if value == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(value)
}

/// Parse `git worktree list --porcelain`.
///
/// The format is blank-line separated blocks of `key value` lines. The shell
/// implementation inferred "skip the main worktree" from an entry counter,
/// which misclassifies as soon as git emits an unexpected block; here each line
/// is interpreted directly and `bare`/`prunable` are first-class.
///
/// Unknown keys are ignored rather than rejected so a future git release that
/// adds metadata does not break parsing.
pub fn parse_worktree_list(input: &str) -> Vec<WorktreeEntry> {
    let mut entries = Vec::new();
    let mut current: Option<WorktreeEntry> = None;

    for raw in input.lines() {
        // Porcelain output is unindented, but hand-written fixtures and some
        // git versions indent attribute lines. Key detection is on the trimmed
        // line so both parse; the value keeps its original text.
        let line = raw.trim_end();
        let key_line = line.trim_start();
        if key_line.is_empty() {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            continue;
        }

        let (key, value) = match key_line.split_once(' ') {
            Some((k, v)) => (k, v),
            // A bare flag with no value (git emits `detached`, `bare`,
            // `prunable` with no argument).
            None => (key_line, ""),
        };

        if key == "worktree" {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(WorktreeEntry {
                path: expand_home(value),
                head: None,
                branch: None,
                bare: false,
                detached: false,
                prunable: false,
            });
        } else {
            let Some(entry) = current.as_mut() else {
                continue;
            };
            match key {
                "HEAD" => entry.head = Sha::parse(value).ok(),
                "branch" => {
                    entry.branch = value
                        .strip_prefix("refs/heads/")
                        .and_then(|name| BranchName::parse(name).ok());
                }
                "bare" => entry.bare = true,
                "detached" => entry.detached = true,
                "prunable" => entry.prunable = true,
                _ => {}
            }
        }
    }

    if let Some(entry) = current {
        entries.push(entry);
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real `git worktree list --porcelain` output: every block opens with
    /// `worktree <path>`, attribute lines are bare flags or `KEY value`, and
    /// blocks are separated by a blank line.
    const REAL_OUTPUT: &str = "\
worktree /Users/me/repo/.bare
bare

worktree /Users/me/repo/worktrees/AX-47383
HEAD 5e44305893b2
branch refs/heads/AX-47383

worktree /Users/me/repo/worktrees/AX-49968
HEAD 611a7b0f031a
branch refs/heads/AX-49968
prunable gitdir file points to non-existent location

worktree /Users/me/repo/.bare/.claude/worktrees/agent-a636779e4451f7a77
HEAD 1505713cd59e
branch refs/heads/worktree-agent-a636779e4451f7a77

worktree /Users/me/repo/worktrees/detached
HEAD abc1234
detached

worktree /Users/me/repo/worktrees/feature-x
HEAD def5678
branch refs/heads/feature/x
";

    #[test]
    fn parses_bare_entry_first() {
        let entries = parse_worktree_list(REAL_OUTPUT);
        assert!(entries[0].bare);
        assert_eq!(entries[0].kind(), gw_core::WorktreeKind::Main);
        assert_eq!(
            entries[0].path,
            std::path::PathBuf::from("/Users/me/repo/.bare")
        );
    }

    #[test]
    fn parses_every_block() {
        let entries = parse_worktree_list(REAL_OUTPUT);
        assert_eq!(entries.len(), 6, "got {entries:#?}");
    }

    #[test]
    fn parses_branch_and_head() {
        let entries = parse_worktree_list(REAL_OUTPUT);
        let wt = &entries[1];
        assert_eq!(wt.branch.as_ref().unwrap().as_str(), "AX-47383");
        assert_eq!(wt.head.as_ref().unwrap().as_str(), "5e44305893b2");
        assert!(!wt.bare);
        assert!(!wt.prunable);
        assert_eq!(wt.kind(), gw_core::WorktreeKind::Linked);
    }

    #[test]
    fn parses_prunable() {
        let entries = parse_worktree_list(REAL_OUTPUT);
        let wt = &entries[2];
        assert!(wt.prunable);
        assert_eq!(wt.kind(), gw_core::WorktreeKind::Prunable);
        assert_eq!(wt.branch.as_ref().unwrap().as_str(), "AX-49968");
    }

    #[test]
    fn harness_worktree_is_recognized() {
        let entries = parse_worktree_list(REAL_OUTPUT);
        assert_eq!(entries[3].kind(), gw_core::WorktreeKind::Harness);
    }

    #[test]
    fn detached_head_has_no_branch() {
        let entries = parse_worktree_list(REAL_OUTPUT);
        let wt = &entries[4];
        assert!(wt.detached);
        assert!(wt.branch.is_none());
        assert!(wt.head.is_some());
        assert_eq!(wt.kind(), gw_core::WorktreeKind::Linked);
    }

    #[test]
    fn slash_branches_are_preserved() {
        let entries = parse_worktree_list(REAL_OUTPUT);
        assert_eq!(entries[5].branch.as_ref().unwrap().as_str(), "feature/x");
    }

    #[test]
    fn empty_input_yields_nothing() {
        assert!(parse_worktree_list("").is_empty());
        assert!(parse_worktree_list("\n\n").is_empty());
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let input = "worktree /repo/wt\nHEAD abc123\nbranch refs/heads/AX-1\nsomething-new yes\n";
        let entries = parse_worktree_list(input);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].branch.as_ref().unwrap().as_str(), "AX-1");
    }

    #[test]
    fn attributes_before_any_worktree_line_are_dropped() {
        assert!(parse_worktree_list("HEAD abc123\nbranch refs/heads/x\n").is_empty());
    }
}
