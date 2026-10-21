//! Local branch cleanup.

use anyhow::Result;

use crate::git;

/// Delete local branches that have been merged into the current branch.
/// The main branch and the current branch are never deleted. A branch that
/// cannot be deleted (for example, one checked out in another worktree) is
/// reported and skipped so the remaining branches are still pruned.
pub fn prune() -> Result<()> {
    let main = git::main_branch()?;
    let current = git::output(&["branch", "--show-current"])?
        .trim()
        .to_owned();

    // A detached HEAD shows up in the list as "(HEAD detached at ...)", which is not a branch.
    let branches: Vec<String> = git::output(&["branch", "--format=%(refname:short)", "--merged"])?
        .lines()
        .map(str::trim)
        .filter(|branch| !branch.is_empty() && !branch.starts_with('('))
        .filter(|branch| *branch != main && *branch != current)
        .map(str::to_owned)
        .collect();

    if branches.is_empty() {
        println!("No merged local branches to prune.");
        return Ok(());
    }

    let mut skipped = Vec::new();
    for branch in branches {
        let outcome = git::attempt(&["branch", "--delete", &branch])?;

        if outcome.status.success() {
            println!("{}", outcome.stdout.trim());
        } else {
            skipped.push((branch, outcome.stderr.trim().to_owned()));
        }
    }

    crate::report_skipped("delete", "branch(es)", &skipped);
    Ok(())
}
