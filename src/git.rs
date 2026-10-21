//! Running Git and reading its results.

use std::process::{Command, ExitStatus, Output};

use anyhow::{Context, Error, Result, anyhow, bail};

/// What a Git command printed and how it exited.
pub struct Outcome {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// Run a Git command, capturing its output. Fails only if Git cannot be started.
pub fn attempt(args: &[&str]) -> Result<Outcome> {
    let Output {
        status,
        stdout,
        stderr,
    } = Command::new("git")
        .args(args)
        .output()
        .context("could not run git; is it installed and on your PATH?")?;

    Ok(Outcome {
        status,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

/// Run a Git command with its output going straight to the terminal, and fail if it fails.
pub fn run(args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .status()
        .context("could not run git; is it installed and on your PATH?")?;

    if !status.success() {
        bail!("git {} failed with {}", args.join(" "), describe(status));
    }

    Ok(())
}

/// Capture a Git command's standard output, surfacing what Git printed if it fails.
pub fn output(args: &[&str]) -> Result<String> {
    let outcome = attempt(args)?;

    if !outcome.status.success() {
        return Err(failure(args, &outcome));
    }

    Ok(outcome.stdout)
}

/// Run a Git command that answers a yes/no question with exit code 0 or 1.
/// Returns the trimmed output on 0 and `None` on 1; any other exit is an error.
fn probe(args: &[&str]) -> Result<Option<String>> {
    let outcome = attempt(args)?;

    match outcome.status.code() {
        Some(0) => Ok(Some(outcome.stdout.trim().to_owned())),
        Some(1) => Ok(None),
        _ => Err(failure(args, &outcome)),
    }
}

/// The name of the repository's main branch.
///
/// `git config kit.mainBranch` wins. Otherwise it is the branch `origin/HEAD`
/// points at, and failing that a local `main` or `master`.
pub fn main_branch() -> Result<String> {
    if let Some(name) = probe(&["config", "--get", "kit.mainBranch"])? {
        if !name.is_empty() {
            return Ok(name);
        }
    }

    if let Some(head) = probe(&[
        "symbolic-ref",
        "--quiet",
        "--short",
        "refs/remotes/origin/HEAD",
    ])? {
        if let Some(name) = head.strip_prefix("origin/") {
            return Ok(name.to_owned());
        }
    }

    for name in ["main", "master"] {
        let reference = format!("refs/heads/{name}");
        if probe(&["show-ref", "--verify", "--quiet", &reference])?.is_some() {
            return Ok(name.to_owned());
        }
    }

    bail!(
        "could not tell which branch is main; run `git remote set-head origin --auto`, \
         or set it with `git config kit.mainBranch <branch>`"
    )
}

/// The error for a failed Git command: what Git printed, or its exit code if it printed nothing.
fn failure(args: &[&str], outcome: &Outcome) -> Error {
    let details = [outcome.stderr.trim(), outcome.stdout.trim()]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    if details.is_empty() {
        anyhow!(
            "git {} failed with {}",
            args.join(" "),
            describe(outcome.status)
        )
    } else {
        anyhow!(details)
    }
}

fn describe(status: ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => "no exit code (terminated by a signal)".to_owned(),
    }
}
