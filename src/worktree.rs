//! Worktree status and cleanup.

use std::fs;

use anyhow::Result;
use serde::Serialize;

use crate::git;

/// One entry from `git worktree list --porcelain`.
#[derive(Debug, Default, PartialEq)]
struct Record {
    path: String,
    head: Option<String>,
    branch: Option<String>,
    is_main: bool,
    bare: bool,
    locked: bool,
    missing: bool,
}

/// A worktree and whether it is safe to remove.
#[derive(Serialize)]
pub struct Status {
    path: String,
    branch: Option<String>,
    merged: bool,
    clean: bool,
    removable: bool,
    note: Option<&'static str>,
}

/// Parse `git worktree list --porcelain` into one record per worktree.
/// The first record is always the main worktree.
fn parse(porcelain: &str) -> Vec<Record> {
    porcelain
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .enumerate()
        .map(|(index, block)| {
            let mut record = Record {
                is_main: index == 0,
                ..Record::default()
            };

            for line in block.lines() {
                let (key, value) = line.split_once(' ').unwrap_or((line, ""));
                match key {
                    "worktree" => record.path = value.to_owned(),
                    "HEAD" => record.head = Some(value.to_owned()),
                    "branch" => {
                        record.branch = Some(
                            value
                                .strip_prefix("refs/heads/")
                                .unwrap_or(value)
                                .to_owned(),
                        );
                    }
                    "bare" => record.bare = true,
                    "locked" => record.locked = true,
                    "prunable" => record.missing = true,
                    _ => {}
                }
            }

            record
        })
        .collect()
}

/// Report every worktree and whether it is fully merged into main and safe to remove.
fn status() -> Result<Vec<Status>> {
    let main = git::main_branch()?;
    let current_path = git::output(&["rev-parse", "--show-toplevel"])?
        .trim()
        .to_owned();
    let porcelain = git::output(&["worktree", "list", "--porcelain"])?.replace("\r\n", "\n");

    parse(&porcelain)
        .into_iter()
        .map(|tree| {
            let merged = match &tree.head {
                Some(head) => git::attempt(&["merge-base", "--is-ancestor", head, &main])?
                    .status
                    .success(),
                None => false,
            };

            let clean = !tree.missing
                && !tree.bare
                && git::output(&["-C", &tree.path, "status", "--porcelain"])?
                    .trim()
                    .is_empty();

            let note = if tree.is_main {
                Some("main worktree")
            } else if same_path(&tree.path, &current_path) {
                Some("current worktree")
            } else if tree.locked {
                Some("locked")
            } else if tree.missing {
                Some("directory missing; run git worktree prune")
            } else if !merged {
                Some("not merged into main")
            } else if !clean {
                Some("uncommitted changes")
            } else {
                None
            };

            Ok(Status {
                path: tree.path,
                branch: tree.branch,
                merged,
                clean,
                removable: note.is_none(),
                note,
            })
        })
        .collect()
}

/// Show each worktree and whether it can be removed, as a table or as JSON.
pub fn list(json: bool) -> Result<()> {
    let trees = status()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&trees)?);
        return Ok(());
    }

    let yes_no = |value: bool| if value { "yes" } else { "no" }.to_owned();
    let rows: Vec<[String; 6]> = trees
        .iter()
        .map(|tree| {
            [
                tree.path.clone(),
                branch_label(tree),
                yes_no(tree.merged),
                yes_no(tree.clean),
                yes_no(tree.removable),
                tree.note.unwrap_or_default().to_owned(),
            ]
        })
        .collect();

    print_table(
        ["PATH", "BRANCH", "MERGED", "CLEAN", "REMOVABLE", "NOTE"],
        &rows,
    );
    Ok(())
}

/// Remove every worktree that is fully merged into main, clean, unlocked, and not the current one.
/// Worktrees that cannot be removed are reported and skipped.
pub fn prune() -> Result<()> {
    let removable: Vec<Status> = status()?
        .into_iter()
        .filter(|tree| tree.removable)
        .collect();

    if removable.is_empty() {
        println!("No merged worktrees to remove.");
        return Ok(());
    }

    let mut skipped = Vec::new();
    for tree in &removable {
        let outcome = git::attempt(&["worktree", "remove", &tree.path])?;

        if outcome.status.success() {
            println!("Removed worktree {} — {}", tree.path, branch_label(tree));
        } else {
            skipped.push((tree.path.clone(), outcome.stderr.trim().to_owned()));
        }
    }

    crate::report_skipped("remove", "worktree(s)", &skipped);
    Ok(())
}

fn branch_label(tree: &Status) -> String {
    tree.branch
        .clone()
        .unwrap_or_else(|| "(detached)".to_owned())
}

/// Whether two paths name the same directory, allowing for symlinks and
/// Windows short names. Falls back to comparing the text if either is missing.
fn same_path(a: &str, b: &str) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn print_table<const N: usize>(header: [&str; N], rows: &[[String; N]]) {
    let mut widths = header.map(|title| title.chars().count());
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }

    let print_row = |cells: [&str; N]| {
        let line: Vec<String> = cells
            .iter()
            .zip(widths)
            .map(|(cell, width)| format!("{cell:<width$}"))
            .collect();
        println!("{}", line.join("  ").trim_end());
    };

    print_row(header);
    for row in rows {
        print_row(row.each_ref().map(String::as_str));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_main_linked_detached_locked_and_prunable_worktrees() {
        let porcelain = "\
worktree /repo
HEAD 1111111111111111111111111111111111111111
branch refs/heads/main

worktree /repo/.worktrees/feature
HEAD 2222222222222222222222222222222222222222
branch refs/heads/feature/login

worktree /repo/.worktrees/detached
HEAD 3333333333333333333333333333333333333333
detached

worktree /repo/.worktrees/locked
HEAD 4444444444444444444444444444444444444444
branch refs/heads/locked
locked on a USB drive

worktree /gone
HEAD 5555555555555555555555555555555555555555
branch refs/heads/gone
prunable gitdir file points to non-existent location
";

        let records = parse(porcelain);

        assert_eq!(records.len(), 5);
        assert_eq!(
            records[0],
            Record {
                path: "/repo".into(),
                head: Some("1111111111111111111111111111111111111111".into()),
                branch: Some("main".into()),
                is_main: true,
                ..Record::default()
            }
        );
        assert_eq!(records[1].branch.as_deref(), Some("feature/login"));
        assert!(!records[1].is_main);
        assert_eq!(records[2].branch, None);
        assert!(records[3].locked);
        assert!(records[4].missing);
    }

    #[test]
    fn parses_a_bare_main_repository() {
        let porcelain = "\
worktree /repo.git
bare

worktree /work/feature
HEAD 2222222222222222222222222222222222222222
branch refs/heads/feature
";

        let records = parse(porcelain);

        assert!(records[0].bare && records[0].is_main);
        assert_eq!(records[0].head, None);
        assert_eq!(records[1].path, "/work/feature");
    }
}
