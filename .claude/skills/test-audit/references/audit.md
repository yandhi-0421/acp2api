# Audit the tests

Sections: the scope, the run directory, what to read, the audit table, the evidence of a deletion, the shape of an edit, the checks.

## The scope

The scope is one of these:

- **The integration branch.** In the workflow, the audit runs once on the branch `implement/<key>`, after all issues of the PRD. The scope holds each test file that the branch added or changed. It also holds each test of the production code that the branch changed.
- **A named scope.** The user names a path, a module, a diff, or a PRD. The scope holds the tests of that path, module, or diff. For a PRD, the scope holds its scenarios, their step definitions, and the tests of the production files that hold the rule of each FR. A test that a finding of `code-review` names is a named scope on the integration branch.
- **A campaign.** The user asks for the whole test surface of one subsystem. Read [campaign.md](campaign.md).

For the integration branch, the base is the merge base of the default branch and the branch. The first command prints the default branch with the prefix `origin/`. If `origin/HEAD` is not set, run the second command, which is a GET. If `origin/<name>` exists, use it. Else use `<name>`.

```sh
git symbolic-ref --short refs/remotes/origin/HEAD
gh api repos/{owner}/{repo} --jq .default_branch
git merge-base <default branch> HEAD
git diff --name-only <base> HEAD
```

A named diff has the base of the diff. A named path, module, or PRD has no base, and a campaign has none. There, each production line in scope counts as changed.

To find the tests of a changed production file, search for its module name and its package name. Search the test files and the step definitions. Write each test file and each production file in scope into `<run>/scope.txt`, with each search command and its hits. A test outside the scope is not audited. If you see a problem in it, record a draft follow-up issue.

Only when the audit edits files, it needs a branch that is not the default branch. On the default branch, first make the branch `test-audit/<scope>`, where `<scope>` is a short name of the scope.

## The run directory

The report goes into `<report dir>`. In the workflow, `<report dir>` is the run directory of `implement`. "The run directory" in [commands.md](../../implement/references/commands.md) of `implement` gives its path. Then `code-review` finds both reports in one place. On another branch, `<report dir>` is `${TMPDIR:-/tmp}/test-audit-<repo>-<branch>`, with each `/` of the branch name as `-`. `<repo>` is the name of the root directory of the repository.

The work files go into `<run>`, which is `<report dir>/test-audit`. Thus they do not replace a file of `implement`. The command below makes `<run>` and prints it:

```sh
b=$(git branch --show-current); [ -n "$b" ] || b=$(git rev-parse --short HEAD); r=$(basename "$(git rev-parse --show-toplevel)"); case "$b" in implement/*) d="${TMPDIR:-/tmp}/implement-$r-${b#implement/}" ;; *) d="${TMPDIR:-/tmp}/test-audit-$r-$(printf '%s' "$b" | tr '/' '-')" ;; esac; mkdir -p "$d/test-audit" && printf '%s\n' "$d/test-audit"
```

At the start of each audit, write these files into `<run>`:

- `status.txt` and `diff.txt`: the output of `git status` and `git diff HEAD`;
- `head.txt`: the output of `git rev-parse HEAD`;
- `start.txt`: the command, the time, and the count of tests of each check, and each failure by name. Also each contract check of [freeze.md](../../bdd/references/freeze.md) that fails, with its issue and the differences that it prints. Run it for each issue with a freeze commit. "Find the freeze commit" in freeze.md lists these issues.

Find the commands of the checks in `CLAUDE.md` or in the CI configuration.

## What to read

Before you judge a test, read these items:

- the whole test, with its tables of cases and its fixtures;
- the production code that it calls, its entry point, its callers, the functions that it calls, and its sibling implementations;
- the sibling tests, and the other tests of the same contract;
- the CI configuration that runs the test;
- the history of the test and of its seam: `git log --follow --format='%h %s' -- <file>`, and the commit that added it.

Keep the discovery read-only. Record the evidence before an edit. Prefer a few suspects with strong evidence to a long list of weak suspects.

## The audit table

The audit table gives each test declaration in scope one row. A test with a table of cases is one row, unless its rows need different marks. Each row has the name and the `file:line` of the test, what it protects, a mark, and one evidence line. Judge a test by its assertions, not by its name.

The marks of step 2 are provisional. Step 3 confirms or changes them with the mutants. The marks:

- `keep`: name the contract. Step 3 adds the mutant that the test kills, or `not mutated`. A test that only moves to a better file stays `keep`, with the move.
- `repair`: keep the contract, but correct the assertion. An example is a check of an absence that passes when only one of several items is absent. A step definition that checks less than the text of its step also gets `repair`.
- `merge`: name the test that takes the assertion first. It can be a row of a table of cases, a stronger suite, or a shared test of the owner.
- `delete`: name the proof that stays, or why no contract exists.

## The evidence of a deletion

Before you delete or merge a test, record each field below. If a field is absent, the suspect is not ready for deletion:

- the name of the test and its `file:line`;
- the failure that it can detect, or the sign of test-behavior-not-implementation that it matches;
- the callers, outside the tests, of the production code or the test support that it covers, and their count;
- the stronger proof that stays, as `file:line`, or why no proof is necessary. It is not a `delete` or `merge` suspect;
- the history: why the test or its seam exists;
- the production code or the test support that the deletion unlocks;
- the risk, and the focused command that validates the change;
- outside the acceptance contract, the result of "A run without the suspects" in [mutation.md](mutation.md).

For a `repair` that changes an expected value, also quote the requirement that changed the behavior. For a test of the acceptance contract, also quote the approval of the user.

## The shape of an edit

- Make one commit for each coherent batch of one owner. Give explicit paths to `git commit`.
- Do not edit or commit a file that holds an uncommitted change of the user. Give its marks in the report as held. Thus a scope of uncommitted changes gives the report only.
- In the body of the commit, give the evidence fields of each test that it deletes or merges. Also give the mutant of each added test, and the quoted requirement of each changed expected value. The temporary directory can be cleared, but the commit stays.
- Move a kept regression test to the owner of its behavior.
- Merge repeated assertions into one table of cases or one shared test.
- Do not add a test that restates the implementation in place of a deleted test.
- Update each place where the project lists its tests or its CI jobs.
- Commit an approved change of the acceptance contract alone. Its body has the line `Approved by the user:` and the quote.
- The contract check of an issue can fail at the start. For a change of that issue, also ask the user to approve those differences. Only when the user approves them, the new freeze commit holds them. Else make no new freeze commit for that issue. Say so in the report.
- For an issue without a freeze commit, make no freeze commit.

Before each commit, apply [protect-secrets-and-personal-data](../../principles/references/protect-secrets-and-personal-data.md). Here it scans the diff and the commit body for secrets and personal data.

Apply [subtract-before-you-add](../../principles/references/subtract-before-you-add.md). Here it deletes each test-only export, global, wrapper, and production path whose only callers were the deleted tests. Do not keep an alias for it.

## The checks

After each commit, run the smallest tests of the owner and of its siblings. If a removed test searched the source text, run the script or the dry run that owns the real contract.

At the end of the audit, also run these commands. The first command must print nothing. The second command gives the line counts of the audit. Give the lines of production code apart from the lines of tests.

```sh
git diff --check <head at the start> HEAD
git diff --numstat <head at the start> HEAD
```
