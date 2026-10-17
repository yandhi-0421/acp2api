# The rerun

Sections: an unfinished merge, unfinished work, a worktree, a second run of one key.

A run that stopped part way leaves state in git and in the run directory. Handle it before the commands that read the done issues, because they switch to the integration branch. Handle an unfinished merge first.

## An unfinished merge

A crash in a merge leaves `MERGE_HEAD` and conflict markers. This command prints a hash if a merge stopped part way:

```sh
git rev-parse -q --verify MERGE_HEAD
```

If it prints a hash, look at the current branch. The merge is yours only on `implement/<key>` or on an issue branch of this key. On another branch, the merge is the merge of the user: ask the user, and name stop reason 3. This command stops the merge only on a branch of this key:

```sh
case "$(git branch --show-current)" in implement/<key>|implement/<key>-issue-*) git merge --abort ;; *) echo "ask the user" ;; esac
```

A worktree can hold a stopped merge too. For each worktree that `git worktree list` shows, run the same two commands with `git -C <worktree>`:

```sh
git -C <worktree> rev-parse -q --verify MERGE_HEAD
case "$(git -C <worktree> branch --show-current)" in implement/<key>|implement/<key>-issue-*) git -C <worktree> merge --abort ;; *) echo "ask the user" ;; esac
```

## Unfinished work

If no merge is unfinished and the current branch is an issue branch of this key, its uncommitted changes come from the run that stopped. Adopt them. Before you switch to another branch, commit them as "Unfinished work" in commands.md says. Their issue keeps the committed work on its branch, and step 3 of `SKILL.md` resumes it when that issue comes up.

## A worktree

A crash in the parallel mode can leave a worktree. Remove the stale entries of the worktrees that no longer exist. In the parallel mode, adopt a worktree directory that exists: give its issue to a subagent in that worktree.

```sh
git worktree prune
git worktree list
```

In the sequential mode, `git switch` fails on a branch that a worktree holds. Run the commands of "Unfinished work" in commands.md in the worktree directory. Then remove the worktree, as "The cleanup" in [parallel.md](parallel.md) says.

## A second run of one key

After the final pull request, the closed sub-issues are not open. A closed issue counts as done for an edge, as "The frontier" in commands.md says.

At the start of a rerun, run the two commands of "No push" in commands.md. If one of them prints a difference, the user pushed since the first run, and this is a second run of this key. Delete `status.txt`, `diff.txt`, `remotes.txt`, `pulls.txt`, `start.txt`, and `report.md` in the run directory. Make the first five again before your first edit, as "The run directory" says. A rerun after an interruption keeps them.

In a second run, the old integration branch can be stale. Only when a merged pull request of this key has the tip of `implement/<key>` as its head, rename the branch. A later run, with new merge commits on the branch, does not rename it. This command is a GET. It prints the merge commit of each such pull request:

```sh
tip=$(git rev-parse implement/<key>)
gh pr list --head 'implement/<key>' --state merged --limit 1000 --json headRefOid,mergeCommit --jq '.[] | "\(.headRefOid) \(.mergeCommit.oid)"' | awk -v t="$tip" '$1 == t {print $2}'
```

If it prints nothing, do not rename the branch. If it prints a hash, check that the default branch holds that merge commit. An unknown object also means that it does not:

```sh
git merge-base --is-ancestor <merge commit> <default branch>
```

If the command fails, ask the user to update the default branch. Name stop reason 3. If it succeeds, keep the old integration branch under a new name. Make a new one from the default branch. The changes in the working tree go with you:

```sh
git branch -m implement/<key> implement/<key>-old-$(git rev-parse --short implement/<key>) && git switch -c implement/<key> <default branch>
```

After this rename, "The base" in commands.md applies to the new branch. If you did not rename the branch and the default branch holds the integration branch, "The base" asks the user for the base.
