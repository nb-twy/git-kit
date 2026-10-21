//! End-to-end tests that run `git-kit` against throwaway repositories.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

/// A temporary directory with its own Git config, so the tests never read the user's.
struct Sandbox {
    dir: TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("create temp dir");
        fs::write(
            dir.path().join("gitconfig"),
            "[user]\n\tname = Test\n\temail = test@example.com\n[commit]\n\tgpgsign = false\n",
        )
        .expect("write gitconfig");
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn command(&self, program: &str, cwd: &Path, args: &[&str]) -> Output {
        Command::new(program)
            .args(args)
            .current_dir(cwd)
            .env("GIT_CONFIG_GLOBAL", self.path("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .unwrap_or_else(|error| panic!("run {program}: {error}"))
    }

    /// Run Git and return its standard output, failing the test if it fails.
    fn git(&self, cwd: &Path, args: &[&str]) -> String {
        let output = self.command("git", cwd, args);
        assert!(
            output.status.success(),
            "git {} failed:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn kit(&self, cwd: &Path, args: &[&str]) -> Output {
        self.command(env!("CARGO_BIN_EXE_git-kit"), cwd, args)
    }

    /// Create a repository with one commit on `branch`.
    fn repo(&self, name: &str, branch: &str) -> PathBuf {
        let path = self.path(name);
        self.git(
            self.dir.path(),
            &["init", "--quiet", "-b", branch, path.to_str().unwrap()],
        );
        self.commit(&path, "initial");
        path
    }

    fn commit(&self, repo: &Path, message: &str) {
        self.git(repo, &["commit", "--quiet", "--allow-empty", "-m", message]);
    }

    fn branches(&self, repo: &Path) -> Vec<String> {
        self.git(repo, &["branch", "--format=%(refname:short)"])
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn branch_prune_keeps_master_when_it_is_the_main_branch() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repo("repo", "master");
    sandbox.git(&repo, &["branch", "old"]);
    sandbox.git(&repo, &["switch", "--quiet", "-c", "feature"]);
    sandbox.commit(&repo, "feature work");

    let output = sandbox.kit(&repo, &["branch", "prune"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(sandbox.branches(&repo), ["feature", "master"]);
}

#[test]
fn branch_prune_uses_kit_main_branch_when_it_cannot_tell() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repo("repo", "trunk");
    sandbox.git(&repo, &["branch", "old"]);
    sandbox.git(&repo, &["switch", "--quiet", "-c", "feature"]);

    let output = sandbox.kit(&repo, &["branch", "prune"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("kit.mainBranch"),
        "{}",
        stderr(&output)
    );

    sandbox.git(&repo, &["config", "kit.mainBranch", "trunk"]);
    let output = sandbox.kit(&repo, &["branch", "prune"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(sandbox.branches(&repo), ["feature", "trunk"]);
}

#[test]
fn branch_prune_reports_branches_it_cannot_delete_and_keeps_going() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repo("repo", "main");
    let held = sandbox.path("held");
    sandbox.git(&repo, &["branch", "done"]);
    sandbox.git(
        &repo,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "held",
            held.to_str().unwrap(),
        ],
    );

    let output = sandbox.kit(&repo, &["branch", "prune"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("Deleted branch done"),
        "{}",
        stdout(&output)
    );
    assert!(
        stderr(&output).contains("Could not delete 1 branch(es):"),
        "{}",
        stderr(&output)
    );
    assert!(stderr(&output).contains("held:"), "{}", stderr(&output));
    assert_eq!(sandbox.branches(&repo), ["held", "main"]);
}

/// A main repository with three linked worktrees: one merged and clean, one
/// with a commit main does not have, and one with uncommitted changes.
fn repo_with_worktrees(sandbox: &Sandbox) -> PathBuf {
    let repo = sandbox.repo("repo", "main");

    for name in ["merged", "ahead", "dirty"] {
        let path = sandbox.path(name);
        sandbox.git(
            &repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                name,
                path.to_str().unwrap(),
            ],
        );
    }
    sandbox.commit(&sandbox.path("ahead"), "unmerged work");
    fs::write(sandbox.path("dirty").join("notes.txt"), "not committed").unwrap();

    repo
}

#[test]
fn worktree_list_reports_which_worktrees_are_removable() {
    let sandbox = Sandbox::new();
    let repo = repo_with_worktrees(&sandbox);

    let output = sandbox.kit(&repo, &["worktree", "list", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));

    let trees: Vec<Value> = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let summary: Vec<(&str, bool, &str)> = trees
        .iter()
        .map(|tree| {
            (
                tree["branch"].as_str().unwrap(),
                tree["removable"].as_bool().unwrap(),
                tree["note"].as_str().unwrap_or(""),
            )
        })
        .collect();

    assert_eq!(
        summary,
        [
            ("main", false, "main worktree"),
            ("ahead", false, "not merged into main"),
            ("dirty", false, "uncommitted changes"),
            ("merged", true, ""),
        ]
    );
}

#[test]
fn worktree_list_never_offers_the_current_worktree() {
    let sandbox = Sandbox::new();
    let repo = repo_with_worktrees(&sandbox);

    let output = sandbox.kit(&sandbox.path("merged"), &["worktree", "list", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));

    let trees: Vec<Value> = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let merged = trees
        .iter()
        .find(|tree| tree["branch"] == "merged")
        .unwrap();
    assert_eq!(merged["removable"], false);
    assert_eq!(merged["note"], "current worktree");
    assert!(repo.exists());
}

#[test]
fn worktree_prune_removes_only_merged_clean_worktrees() {
    let sandbox = Sandbox::new();
    let repo = repo_with_worktrees(&sandbox);

    let output = sandbox.kit(&repo, &["worktree", "prune"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("Removed worktree"),
        "{}",
        stdout(&output)
    );
    assert!(!sandbox.path("merged").exists());
    assert!(sandbox.path("ahead").exists());
    assert!(sandbox.path("dirty").exists());
    // Branches stay until `branch prune`.
    assert!(sandbox.branches(&repo).contains(&"merged".to_owned()));
}

#[test]
fn session_end_updates_main_then_removes_merged_worktrees_and_branches() {
    let sandbox = Sandbox::new();

    // A remote whose default branch is `trunk`, and a second clone standing in for GitHub merges.
    let origin = sandbox.path("origin.git");
    sandbox.git(
        sandbox.dir.path(),
        &[
            "init",
            "--quiet",
            "--bare",
            "-b",
            "trunk",
            origin.to_str().unwrap(),
        ],
    );
    let upstream = sandbox.path("upstream");
    sandbox.git(
        sandbox.dir.path(),
        &[
            "clone",
            "--quiet",
            origin.to_str().unwrap(),
            upstream.to_str().unwrap(),
        ],
    );
    sandbox.commit(&upstream, "initial");
    sandbox.git(&upstream, &["push", "--quiet", "origin", "trunk"]);

    let work = sandbox.path("work");
    sandbox.git(
        sandbox.dir.path(),
        &[
            "clone",
            "--quiet",
            origin.to_str().unwrap(),
            work.to_str().unwrap(),
        ],
    );
    let feature = sandbox.path("work-feature");
    sandbox.git(
        &work,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "feature",
            feature.to_str().unwrap(),
        ],
    );
    sandbox.commit(&feature, "feature work");
    sandbox.git(&feature, &["push", "--quiet", "origin", "feature"]);

    // Merge the feature on the remote, then leave the main clone on another branch.
    sandbox.git(
        &upstream,
        &["pull", "--quiet", "--ff-only", "origin", "feature"],
    );
    sandbox.git(&upstream, &["push", "--quiet", "origin", "trunk"]);
    sandbox.git(&work, &["switch", "--quiet", "-c", "scratch"]);

    let output = sandbox.kit(&work, &["session", "end"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        sandbox.git(&work, &["branch", "--show-current"]).trim(),
        "trunk"
    );
    assert_eq!(
        sandbox.git(&work, &["log", "-1", "--format=%s"]).trim(),
        "feature work"
    );
    assert!(!feature.exists());
    assert_eq!(sandbox.branches(&work), ["trunk"]);
}
