# Set the scope and the intent

Sections: the kinds of change, the revisions, each path in scope, the intent sources.

Set one review scope before you judge the change. Record the base, the head, and the path set, so that another reviewer can reproduce the scope.

## The kinds of change

- **A named pull request.** Get the fixed base and head revisions from read-only GitHub data or from local objects. Use the comparison of GitHub. If it is not available, compare the merge base with the fixed head. Do not use the local `HEAD` in place of the head of the pull request.

  If the head or the base is not local, fetch both with `git fetch origin pull/<n>/head <base branch>`. `<base branch>` is the base branch of the pull request. The fetch makes no branch. If `isCrossRepository` of `gh pr view` is true, apply [guard-one-way-doors](../../principles/references/guard-one-way-doors.md). Here it makes each run of the code of the pull request or of a lever wait for the yes of the user. Put the question into the report.
- **A committed branch.** Use the merge base of the target branch and the head of the branch as the diff base. Compare that base with the head of the branch. Disclose the unrelated changes of the working tree, and keep them out of the scope.
- **The integration branch of the workflow.** It is the committed branch `implement/<key>`. Its target branch is the base that the report of `implement` gives.
- **Uncommitted changes.** Review the staged changes against `HEAD`, and the unstaged changes against the index. Include the untracked files that git does not ignore. Obey a request for the staged changes only, the unstaged changes only, or some paths only. Exclude the earlier commits of the branch.
- **A branch and the working tree.** Only when the user asks for this combined scope, include the committed and the uncommitted changes. Compare the merge base of the target branch with the working tree. Examine the staged changes separately.
- **A change since a named revision.** Use that revision as the base of the comparison, and resolve the requested end. Only for an explicit request to compare a contribution, use a merge base.

## The revisions

Resolve only the revisions that the scope needs. A comparison of a contribution needs a target branch from the request of the user, the pull request, or GitHub data. Only when the data of the repository shows that a configured upstream has that role, use it. Only when a necessary end stays ambiguous, ask the user (stop reason 3). Do not ask for a target branch that the scope does not need. If no source names a target branch, use the default branch, and say so in the report.

Resolve fixed commit IDs for each committed scope. For the comparison of a contribution, compute `DIFF_BASE` with `git merge-base`. For a change since a named revision, use `BASE_TIP` as `DIFF_BASE`. This example shows the comparison of a contribution:

```sh
BASE_TIP=$(git rev-parse --verify "$BASE_REF^{commit}")
HEAD_TIP=$(git rev-parse --verify "$HEAD_REF^{commit}")
DIFF_BASE=$(git merge-base "$BASE_TIP" "$HEAD_TIP")
git diff --name-status -M "$DIFF_BASE" "$HEAD_TIP"
git diff --numstat "$DIFF_BASE" "$HEAD_TIP"
git log --oneline "$DIFF_BASE..$HEAD_TIP"
```

If a revision does not resolve, or the scope is empty, stop at this step, before a subagent starts.

For uncommitted work, examine the staged and the unstaged diffs separately. In a combined diff, two opposite edits can disappear.

```sh
git diff --cached --name-status
git diff --name-status
git --no-optional-locks status --porcelain=v1 --untracked-files=all
```

Record the `HEAD` commit and the output of these three commands, so that another reviewer can compare. Use the full diff of each changed path. For a requested scope of a branch and the working tree, also compare the working tree with its merge base. Keep the untracked files apart from the tracked diffs.

For the staged changes only, the head of the scope is the index. Read a file there with `git show ':<path>'`. Make an isolated copy as guard-one-way-doors says for the staged changes only.

## Each path in scope

Account for each path in scope: renames, deletions, binary files, generated files, vendor files, migrations, dependencies, and delivery configuration. Examine each path as deeply as its effect needs. A scope is empty only when each diff and each set of untracked files of that scope is empty.

## The intent sources

Find each source with the [document map](../../domain-modeling/references/document-map.md). Use the sources in this order of authority:

1. The requirement of the user: the request, the PRD, and the scenarios with their FR tags.
2. The pull request and the issues that it links: the parent issue and its sub-issues, through `docs/agents/issue-tracker.md`.
3. A named design or plan: the written design and the accepted ADRs.
4. The commit messages and the notes of the task, for example the reports of `implement`, `no-comments`, and `test-audit`. The paragraphs after this list give their places.
5. The tests and the nearby behavior, as evidence of the current contract.

The report of `implement` is the file `report.md` in its run directory. "The run directory" in [commands.md](../../implement/references/commands.md) of `implement` gives its path. `<repo>` is the name of the root directory of the repository.

The reports of `no-comments` and `test-audit` are in the files `no-comments.md` and `test-audit.md` in that directory. On another branch, each file is in `${TMPDIR:-/tmp}/<skill>-<repo>-<branch>`, with the name of its skill as `<skill>`, and each `/` of the branch name as `-`. The first line of each report names the head at the end of its run. A report covers only the commits up to that head. In each of these cases, ignore a report:

- `git merge-base --is-ancestor <its head> <head>` fails;
- `git merge-base --is-ancestor <its head> <base>` succeeds.

For a scope of uncommitted changes, `<base>` and `<head>` are both `HEAD`, so the second case ignores each report. There, use each report whose head is `HEAD` as a lead only. The working tree can have changed after its run. Check each claim of such a report against the current diff.

Read each source that exists. Report each conflict between sources. Do not prefer a source because it matches the implementation.

An absent source is not a question to the user. Put the gap under `Unknowns`. If `docs/agents/issue-tracker.md` does not exist, continue without the issues, and say so under `Unknowns`.

Apply question 4 of the self-check of [sequence-verifiable-units](../../principles/references/sequence-verifiable-units.md). Here it maps the requirements and the delivered behavior of the whole scope, not of one issue. Mark the work outside the repository as unproven.

The scope is ready when you can tell these apart:

- the requested behavior;
- the delivered behavior;
- the extra behavior;
- the conflicts;
- the work that is not verified.
