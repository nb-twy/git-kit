//! `git kit`: Git helpers for ending a coding session.
//!
//! Git runs any `git-<name>` program on the PATH as `git <name>`, so this binary
//! works as `git kit ...` from every shell.

mod branch;
mod git;
mod worktree;

use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "git-kit",
    bin_name = "git kit",
    version,
    about,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Clean up local branches.
    #[command(arg_required_else_help = true)]
    Branch {
        #[command(subcommand)]
        command: BranchCommand,
    },
    /// Inspect and clean up worktrees.
    #[command(arg_required_else_help = true)]
    Worktree {
        #[command(subcommand)]
        command: WorktreeCommand,
    },
    /// Keep the main branch current.
    #[command(arg_required_else_help = true)]
    Main {
        #[command(subcommand)]
        command: MainCommand,
    },
    /// Wrap up a coding session.
    #[command(arg_required_else_help = true)]
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
}

#[derive(Subcommand)]
enum BranchCommand {
    /// Delete local branches that have been merged into the current branch.
    ///
    /// The main branch and the current branch are never deleted. A branch that
    /// cannot be deleted (for example, one checked out in another worktree) is
    /// reported and skipped so the remaining branches are still pruned.
    Prune,
}

#[derive(Subcommand)]
enum WorktreeCommand {
    /// Show each worktree and whether it is fully merged into main and can be removed.
    List {
        /// Print the worktrees as JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Remove every worktree that is merged into main, clean, unlocked, and not the current one.
    ///
    /// Branches are left in place; run `git kit branch prune` afterward.
    /// Worktrees that cannot be removed are reported and skipped.
    Prune,
}

#[derive(Subcommand)]
enum MainCommand {
    /// Switch to the main branch and fast-forward it from its upstream, pruning stale remote refs.
    Update,
}

#[derive(Subcommand)]
enum SessionCommand {
    /// Update main, remove merged worktrees, then prune merged local branches.
    ///
    /// Worktrees go first so the branches they held can be pruned in the same pass.
    End,
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<()> {
    match command {
        Command::Branch {
            command: BranchCommand::Prune,
        } => branch::prune(),
        Command::Worktree {
            command: WorktreeCommand::List { json },
        } => worktree::list(json),
        Command::Worktree {
            command: WorktreeCommand::Prune,
        } => worktree::prune(),
        Command::Main {
            command: MainCommand::Update,
        } => update_main(),
        Command::Session {
            command: SessionCommand::End,
        } => {
            update_main()?;
            worktree::prune()?;
            branch::prune()
        }
    }
}

fn update_main() -> Result<()> {
    let main = git::main_branch()?;
    git::run(&["switch", &main])?;
    git::run(&["pull", "--prune", "--ff-only"])
}

/// Print the items a prune skipped, and why, to standard error.
fn report_skipped(action: &str, noun: &str, skipped: &[(String, String)]) {
    if skipped.is_empty() {
        return;
    }

    eprintln!("Could not {action} {} {noun}:", skipped.len());
    for (item, reason) in skipped {
        eprintln!("  {item}: {reason}");
    }
}
