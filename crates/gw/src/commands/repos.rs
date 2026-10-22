use std::path::{Path, PathBuf};

use gw_core::render::human::short_age;

use crate::cli::ReposArgs;
use crate::config::now_epoch;
use crate::error::Error;
use crate::git::{CliGit, GitBackend};
use crate::output::{layout_rows, Outcome};
use crate::repos;

/// One layout, as reported by `gw repos`.
struct RepoRow {
    slug: String,
    path: PathBuf,
    worktrees: usize,
    /// Present only when `--dirty` asked for it.
    dirty: Option<usize>,
    last_commit_age_secs: Option<i64>,
    /// Ticket providers the layout's own convention names.
    ticket_providers: Vec<String>,
    /// Project filters its ticket rules declare.
    jira_projects: Vec<String>,
}

pub fn run(args: &ReposArgs, json: bool) -> Result<Outcome, Error> {
    let root = repos::repos_root()?;
    let mut rows = Vec::new();

    for (slug, path) in repos::discover(&root) {
        match inspect(&slug, &path, args.dirty) {
            Ok(row) => rows.push(row),
            Err(e) => eprintln!("warning: {slug}: {e}"),
        }
    }

    if json {
        print_json(&rows);
        return Ok(Outcome::ok());
    }

    if rows.is_empty() {
        println!("No layouts under {}", root.display());
        return Ok(Outcome::ok());
    }

    println!("Layouts under {}", root.display());
    println!();
    print_table(&rows, args.dirty);
    Ok(Outcome::ok())
}

fn inspect(slug: &str, root: &Path, want_dirty: bool) -> Result<RepoRow, Error> {
    let git = CliGit::new(root.join(".bare"));
    let now = now_epoch();
    let mut worktrees = 0;
    let mut dirty = 0;
    let mut newest: Option<i64> = None;

    for entry in git.worktree_list()? {
        if entry.bare || gw_core::Layout::is_harness_worktree(&entry.path) {
            continue;
        }
        worktrees += 1;

        if want_dirty {
            // A failed probe must not hide a dirty tree, but it also must not
            // abort the whole listing; count it, and let the count be a floor.
            match git.status(&entry.path) {
                Ok(state) if !state.is_clean() => dirty += 1,
                Ok(_) => {}
                Err(_) => dirty += 1,
            }
        }

        if let Ok(Some(epoch)) = git.last_commit_epoch(&entry.path) {
            newest = Some(newest.map_or(epoch, |current| current.max(epoch)));
        }
    }

    let jira_projects = match crate::settings::declared_projects(root) {
        Ok(projects) => projects,
        Err(e) => {
            eprintln!("warning: {slug}: {e}");
            Vec::new()
        }
    };
    let ticket_providers = match crate::settings::declared_ticket_providers(root) {
        Ok(providers) => providers,
        Err(e) => {
            eprintln!("warning: {slug}: {e}");
            Vec::new()
        }
    };

    Ok(RepoRow {
        slug: slug.to_string(),
        path: root.to_path_buf(),
        worktrees,
        dirty: want_dirty.then_some(dirty),
        last_commit_age_secs: newest.map(|epoch| (now - epoch).max(0)),
        ticket_providers,
        jira_projects,
    })
}

fn print_json(rows: &[RepoRow]) {
    #[derive(serde::Serialize)]
    struct JsonRow<'a> {
        repo: &'a str,
        path: String,
        worktrees: usize,
        dirty: Option<usize>,
        last_commit_age_secs: Option<i64>,
        ticket_providers: &'a [String],
        jira_projects: &'a [String],
    }

    let rows: Vec<JsonRow<'_>> = rows
        .iter()
        .map(|row| JsonRow {
            repo: &row.slug,
            path: row.path.display().to_string(),
            worktrees: row.worktrees,
            dirty: row.dirty,
            last_commit_age_secs: row.last_commit_age_secs,
            ticket_providers: &row.ticket_providers,
            jira_projects: &row.jira_projects,
        })
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({ "repos": rows }))
            .unwrap_or_else(|_| "{}".into())
    );
}

fn print_table(rows: &[RepoRow], with_dirty: bool) {
    let last_commit = |row: &RepoRow| {
        row.last_commit_age_secs
            .map_or_else(|| "—".to_string(), short_age)
    };
    // Show the provider and its project filter together, so a layout that
    // uses Linear is not labelled "JIRA".
    let ticket_column = |row: &RepoRow| match (
        row.ticket_providers.is_empty(),
        row.jira_projects.is_empty(),
    ) {
        (true, _) => "—".to_string(),
        (false, true) => row.ticket_providers.join(","),
        (false, false) => format!(
            "{}:{}",
            row.ticket_providers.join(","),
            row.jira_projects.join(",")
        ),
    };

    if with_dirty {
        let mut table: Vec<[String; 5]> = vec![heading([
            "REPO",
            "WORKTREES",
            "DIRTY",
            "LAST COMMIT",
            "TICKETS",
        ])];
        for row in rows {
            table.push([
                row.slug.clone(),
                row.worktrees.to_string(),
                row.dirty.unwrap_or(0).to_string(),
                last_commit(row),
                ticket_column(row),
            ]);
        }
        println!("{}", layout_rows(&table));
    } else {
        let mut table: Vec<[String; 4]> =
            vec![heading(["REPO", "WORKTREES", "LAST COMMIT", "TICKETS"])];
        for row in rows {
            table.push([
                row.slug.clone(),
                row.worktrees.to_string(),
                last_commit(row),
                ticket_column(row),
            ]);
        }
        println!("{}", layout_rows(&table));
    }
}

fn heading<const N: usize>(cells: [&str; N]) -> [String; N] {
    cells.map(str::to_string)
}
