# git-kit

Git helpers for ending a coding session: update the main branch, remove
worktrees that have been merged, and delete merged local branches.

`git-kit` is a single binary. Git runs any `git-<name>` program on your `PATH`
as `git <name>`, so once it is installed you use it as `git kit` from any
shell: bash, zsh, fish, PowerShell, or Nushell.

## Install

macOS and Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/nb-twy/git-kit/releases/latest/download/git-kit-installer.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/nb-twy/git-kit/releases/latest/download/git-kit-installer.ps1 | iex"
```

The installers put `git-kit` in `~/.local/bin` and add that directory to your
`PATH` if needed. You can also download an archive for your platform from the
[releases page](https://github.com/nb-twy/git-kit/releases), or build from
source with `cargo install --git https://github.com/nb-twy/git-kit`.

## Use

```sh
# Update main, remove merged worktrees, then delete merged local branches
git kit session end

# Switch to main and fast-forward it from its upstream, pruning stale remote refs
git kit main update

# Show every worktree and whether it is merged into main and removable
git kit worktree list

# Remove every worktree that is merged, clean, unlocked, and not the current one
git kit worktree prune

# Delete local branches merged into the current branch; any it cannot delete
# (for example, a branch checked out in another worktree) are reported and the
# rest are still pruned
git kit branch prune
```

For help, use `git kit -h` or `git kit help <command>`. `git kit --help` makes
Git look for a `git-kit` man page, which isn't installed.

A worktree is removable only when its `HEAD` is an ancestor of main, it has no
uncommitted or untracked files, it is not locked, and it is not the main or
current worktree. `git kit worktree prune` does not delete branches; run
`git kit branch prune` afterward (`git kit session end` does both). Merge
detection uses Git ancestry, so branches that were squash-merged are not
recognized as merged.

### Which branch is main

git-kit never deletes the main branch. It works out which branch that is in
this order:

1. `git config kit.mainBranch`, if you have set it.
2. The branch `origin/HEAD` points at, which `git clone` sets to the remote's
   default branch.
3. A local `main`, then a local `master`.

If none of those apply, git-kit stops and asks you to run
`git remote set-head origin --auto` or `git config kit.mainBranch <branch>`.

### JSON output

`git kit worktree list --json` prints the worktrees as JSON, for scripts. In
Nushell, `git kit worktree list --json | from json` gives you a table.

## Develop

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

The tests create throwaway repositories in temporary directories and run the
built binary against them, with Git's global and system config switched off.

### Pull requests

`main` only changes through pull requests. Each PR runs two workflows, and
both must pass before it can merge:

- **CI** (`ci.yml`): rustfmt and clippy, the tests on Linux, macOS, and
  Windows, and a check on the version in `Cargo.toml`. The `ci-ok` job passes
  only when all of them pass.
- **Release** (`release.yml`, in dry-run mode): cargo-dist builds the release
  archives and installers for every target, exactly as a release would. The
  builds are attached to the workflow run if you want to try a PR's binary.

## Release

`Cargo.toml` holds the version, and merging a version bump publishes it.

1. In a PR, bump `version` in `Cargo.toml` and run `cargo check` so
   `Cargo.lock` picks up the new version (CI builds with `--locked`). The
   version check reports "Merging this PR will release vX.Y.Z", and fails if
   the version is older than the latest release.
2. Merge the PR. The **Release on version bump** workflow sees a version with
   no tag yet and starts the Release workflow, which builds every target,
   creates the `vX.Y.Z` tag on that commit, and publishes the GitHub release
   with the installers.

PRs that leave the version alone release nothing; their changes go out with
the next bump. To retry a release that failed, run
`gh workflow run release.yml -f tag=vX.Y.Z`, or merge the fix: the next merge
to `main` starts the release again while the tag is still missing.

Releases are built by [cargo-dist](https://github.com/axodotdev/cargo-dist).
Its settings are in `dist-workspace.toml`, and `dist generate` writes
`.github/workflows/release.yml` from them; don't edit that workflow by hand.
Run `dist plan` to preview what a release will contain, and `dist init` after
upgrading cargo-dist.
