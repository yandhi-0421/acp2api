# The commands

Sections:

- the placeholders;
- the run directory;
- read the issues;
- the frontier;
- the base;
- the done issues;
- the start;
- the issue branch;
- unfinished work;
- the secret scan;
- the merge;
- a conflict;
- no push.

Run each command from the root of the repository. Step 1 of `SKILL.md` uses "The base" and "The done issues". Step 2 uses "The run directory" and "The start". Step 3 uses "The issue branch".

## The placeholders

Replace each placeholder before you run a command:

- `<key>` is the title of the parent issue.
- `<path>` is the path of the PRD, or of the written design in the improve mode.
- `<design path>` is the path of the written design. It has no value if the project has none.
- `<parent>` is the number of the parent issue.
- `<n>` is the number of the issue that you work.
- `<dir>` is the feature directory of the PRD. It has no value in the improve mode.
- `<title>` is the title of the issue that you work.
- `<plan paths>` are the plan documents that exist and that `git -c core.quotepath=off status --short` lists. Git adds quotes around a path with a space. Take the text inside the quotes, and quote the path for the shell.
- `<default branch>` is the default branch as a git ref: `origin/<name>` if it exists, else `<name>`.
- `<base>` is the hash that the base commands print.
- `<diff command>` is the command that prints the text to scan.
- `<report file>` is `<run>/report.md`.
- `<worktree>` is the directory of a worktree.
- `{owner}` and `{repo}` stay as they are, because `gh` fills them from the git remote.

## The run directory

`<run>` is the directory `${TMPDIR:-/tmp}/implement-<repo>-<key>`. `<repo>` is the name of the root directory of the repository, so two projects with one key stay apart. The directory is outside the repository, so the brief is outside the write scope of a subagent. The key decides its name, so a rerun finds it again.

It holds `status.txt`, `diff.txt`, `remotes.txt`, and `pulls.txt` (the state before your first edit), `start.txt`, and one brief for each issue. If one of these five files exists, keep it, except in a second run of one key.

`start.txt` holds the command and the count of tests of each check, and each failure by name. The count is the number of tests that run, with the `@wip` scenarios left out. If the runner can list the names of its tests, the file also keeps the names. The file leaves out the undefined steps of `@wip` scenarios. A failure of a scenario of the issue is never a failure from the start.

If neither `CLAUDE.md` nor the CI configuration names the command of a check, ask the user. Name stop reason 3.

Make the directory and save the state. The command prints `<run>`:

```sh
w="${TMPDIR:-/tmp}/implement-$(basename "$(git rev-parse --show-toplevel)")-<key>"; mkdir -p "$w" && { [ -e "$w/status.txt" ] || git status > "$w/status.txt"; } && { [ -e "$w/diff.txt" ] || git diff HEAD > "$w/diff.txt"; } && { [ -e "$w/remotes.txt" ] || git for-each-ref 'refs/remotes/*/implement/<key>' 'refs/remotes/*/implement/<key>-*' > "$w/remotes.txt"; }; printf '%s\n' "$w"
```

Then record the count of the pull requests of this key. This command is a GET. It counts each pull request with this head branch, also from a fork:

```sh
[ -e "<run>/pulls.txt" ] || { n=$(gh pr list --head 'implement/<key>' --state all --limit 1000 --json number --jq length) && printf '%s\n' "$n" > "<run>/pulls.txt"; }
```

## Read the issues

Find the parent issue with the command of "Find the parent issue" in `docs/agents/issue-tracker.md`. There, `<title>` is `<key>`. If two matches are open, ask the user which one to use. Name stop reason 3.

This command reads the sub-issues as "Read the sub-issues" in the tracker file says, with a filter of its own. It prints one line for each sub-issue: number, state, labels, kind, text edges, and title. The kind is `slice` for the heading `## Scenarios`, and `refactoring` for the heading `## What to change`. A text edge is a line `Blocked by: #<number>` in the body.

```sh
gh api --paginate repos/{owner}/{repo}/issues/<parent>/sub_issues --jq '.[] | [.number, .state, ([.labels[].name] | join(",")), (if (.body // "") | contains("## Scenarios") then "slice" elif (.body // "") | contains("## What to change") then "refactoring" else "unknown" end), ([(.body // "") | scan("Blocked by: #([0-9]+)") | .[0]] | join(",")), .title] | @tsv'
```

Read the API edges of each sub-issue with the loop of "Read the blocking edges" in the tracker file. It prints `unverified` for a read that fails.

To list the scenarios that no issue holds, print each scenario whose tag line has no slice tag. They are in the scope of no issue:

```sh
find <dir> -name '*.feature' -exec awk '
FNR==1{prev=""}
/^[[:space:]]*(Scenario|Scenario Outline|Scenario Template|Example):/ && prev !~ /@issue-[0-9]+/ {print FILENAME ": " $0}
{prev=$0}' {} +
```

## The frontier

The frontier is every sub-issue that meets all of these rules:

- it is open and not done, and it did not fail or halt in this run;
- it has no label `ready-for-human`, because that label means that the issue needs a decision, a check, or a date of the user;
- its kind is `slice` or `refactoring`;
- it has no edge that is `unverified`;
- each edge into it comes from a done issue or from a closed issue.

If the request names issues, the frontier holds only them, and the label `ready-for-human` does not exclude them. A named issue enters the frontier after each named issue that blocks it is done. If a named issue fails or halts, each named issue that it blocks halts too.

Step 3 of `SKILL.md` never takes an issue outside the frontier. List each other open sub-issue in the plan with its reason.

## The base

If the integration branch does not exist, skip this section and the next section. Find the default branch. The first command prints it with the prefix `origin/`. If it fails because `origin/HEAD` is not set, run the second command, which is a GET. If `origin/<name>` exists, use it. Else use `<name>`.

```sh
git symbolic-ref --short refs/remotes/origin/HEAD
gh api repos/{owner}/{repo} --jq .default_branch
```

The base of the integration branch is its merge-base with the default branch. A reflog expires, so do not use it. If the run started on another branch, the base is the commit where that branch left the default branch. Give the base in the report:

```sh
git merge-base implement/<key> <default branch>
```

Run the next three commands. The default branch holds the integration branch already when all of these are true:

- the first command succeeds;
- the two hashes of the second command differ;
- the third command prints the number of a sub-issue of the parent issue.

In that case, report it, and ask the user for the base. Name stop reason 3. In each other case, the base is the hash that the merge-base command prints.

```sh
git merge-base --is-ancestor implement/<key> <default branch>
git rev-parse implement/<key> <default branch>
git log --first-parent --format=%s implement/<key> | grep -oE '^Merge issue #[0-9]+' | grep -oE '[0-9]+' | sort -un
```

## The done issues

Step 1 of `SKILL.md` defines a done issue and a reopened issue. A reopened issue is not a done issue. If the integration branch does not exist, no issue is done.

Make the run directory first. Handle the rerun states of [rerun.md](rerun.md). Then switch to the integration branch with the command of "The start". If the switch fails, ask the user. Name stop reason 3.

The command below reads the feature files of the working tree, and only the merges after the base. It prints `done` or `reopened` and the number of each merged issue. Keep only the numbers of the sub-issues of the parent issue.

```sh
for n in $(git log --first-parent --format=%s <base>..implement/<key> | grep -oE '^Merge issue #[0-9]+' | grep -oE '[0-9]+' | sort -un); do
  if grep -rE "@issue-$n([[:space:]]|\$)" <dir> | grep -qE '@wip([[:space:]]|$)'; then printf 'reopened\t%s\n' "$n"; else printf 'done\t%s\n' "$n"; fi
done
```

In the improve mode, each merged issue is done, because no scenario exists. This command prints the numbers:

```sh
git log --first-parent --format=%s <base>..implement/<key> | grep -oE '^Merge issue #[0-9]+' | grep -oE '[0-9]+' | sort -un
```

A reopened issue gets a new freeze commit from its new `bdd` run. The lookup in [freeze.md](../../bdd/references/freeze.md) finds the newest freeze commit that HEAD can reach.

## The start

The run mode is sequential, or parallel when the user asks for it. In the sequential mode, do not make a worktree. A new worktree lacks the ignored files of the project, for example installed dependencies and `.env`.

The plan documents are the PRD, the feature files, the written design, `GLOSSARY.md`, `GLOSSARY-MAP.md`, the ADRs in `docs/adr/`, `docs/agents/issue-tracker.md`, `.claude/skills/`, `.claude/workflow-manifest.json`, and `CLAUDE.md`. This command prints each uncommitted change outside them. The output must be empty. If it is not, ask the user:

```sh
git status --short -- . ':(exclude)<path>' ':(exclude)<design path>' ':(exclude)<dir>' ':(exclude)GLOSSARY.md' ':(exclude)GLOSSARY-MAP.md' ':(exclude)docs/adr' ':(exclude)docs/agents/issue-tracker.md' ':(exclude).claude/skills' ':(exclude).claude/workflow-manifest.json' ':(exclude)CLAUDE.md'
```

Leave out each `:(exclude)` item whose placeholder has no value, for example `<design path>` when no written design exists. An empty `':(exclude)'` hides every change.

Make the integration branch from the current commit. If it exists, switch to it:

```sh
if git rev-parse --verify --quiet refs/heads/implement/<key> >/dev/null; then git switch implement/<key>; else git switch -c implement/<key>; fi
```

If `git status --short -- <plan paths>` prints a line, commit exactly these paths on the integration branch. Never use `git add -A`, because it adds the changes of the user. Stage the paths first:

```sh
git add -- <plan paths>
```

Run the secret scan on the staged changes. Then commit:

```sh
git commit -m "Add the plan documents of <key>" -- <plan paths>
```

Then run the commands of "The base", and give the base in the report. Then run the contract check of each done slice issue, as "An approved change" in [check.md](check.md) says.

## The issue branch

Make the issue branch from the tip of the integration branch. If the branch exists, switch to it. If it lacks the tip, merge the tip into it. If that merge conflicts, follow "A conflict". A reopened issue has an issue branch that is behind the tip, so this brings it to the tip:

```sh
if git rev-parse --verify --quiet refs/heads/implement/<key>-issue-<n> >/dev/null; then git switch implement/<key>-issue-<n> && { git merge-base --is-ancestor implement/<key> HEAD || git merge --no-edit implement/<key>; }; else git switch -c implement/<key>-issue-<n> implement/<key>; fi
```

## Unfinished work

The unfinished work of an issue is the uncommitted change on its issue branch. It comes from a subagent that stopped or failed. Stage every changed path with an explicit list. The list is NUL-separated, so a path with a space or a non-ASCII character is safe:

```sh
git -c core.quotepath=off ls-files -z --modified --others --deleted --exclude-standard | xargs -0 git add --
```

Run the secret scan on the staged changes. If the scan finds a secret, do not commit, as "The secret scan" says. Otherwise commit:

```sh
git commit -m "Save the unfinished work of issue #<n>"
```

## The secret scan

Use the secret scanner of the project. If it has none, run this command. It counts the added lines that look like a secret. The evidence is `0`. Set `<diff command>` as follows:

- before a commit, `git diff --cached`;
- before a merge, `git log -p --format= implement/<key>..HEAD`, which scans each commit of the issue branch;
- before the report, `sed 's/^/+/' <report file>`, because the command reads only added lines.

```sh
<diff command> | grep -E '^\+' | grep -ciE -e '-----BEGIN [A-Z ]*PRIVATE KEY' -e '(password|secret|token|api_?key)[a-z_]*[^[:alnum:]_=:]*[:=]' -e '[a-z]+://[^[:space:]/@:]+:[^[:space:]/@]+@' -e 'Bearer [A-Za-z0-9._~+/-]{20,}' -e 'Basic [A-Za-z0-9+/=]{16,}' -e '(^|[^A-Za-z0-9_])(gh[pousr]_|github_pat_|AKIA|sk-|sk_(live|test)_|xox[abprs]-)[A-Za-z0-9_-]{16,}'
```

If the count is above 0, find each hit in the diff output or in the files. Read it there. Do not copy a secret into the report. A hit that is not a secret, for example `password: string;`, goes into the report. A clean scan exits with status 1, because `grep -c` does so for a count of 0. Do not chain it with `&&`.

If a hit is a secret in an uncommitted file, do not commit it. Keep the file out of each commit: unstage it with `git reset -q -- <path>`. Do not switch branches while the file is in the working tree. Name its path in the report. Ask the user, and name stop reason 3, because only a revoked secret is safe.

If the secret is in a commit of the issue branch, the issue fails and gets no retry.

## The merge

The merge commit is the record that the issue is done. Its first line is `Merge issue #<n>: <title of the issue>`. If the title has a quote, a dollar sign, or a backtick, write the message to a file. Then use `-F <file>`.

If the first command prints `0`, the issue branch has no commit, and the merge makes no commit. If a command shows each "Done when" line, make an empty commit with the same first line. Name it in the report. If a line is unproven, the issue fails. If the count is above 0, run the merge command.

```sh
git rev-list --count implement/<key>..implement/<key>-issue-<n>
```

```sh
git switch implement/<key> && git commit --allow-empty -m "Merge issue #<n>: <title>"
```

```sh
git switch implement/<key> && git merge --no-ff -m "Merge issue #<n>: <title>" implement/<key>-issue-<n>
```

Read the merge back. The first line must show the merge commit of the issue:

```sh
git log --first-parent -1 --format='%h %s'
```

## A conflict

If the merge of the tip into the issue branch conflicts, never resolve the conflict by hand. Run this command. It stops the merge, keeps the old branch under a new name with its short hash, and makes the issue branch again from the tip. Then give the issue to a new subagent. This is not the retry of check.md.

```sh
git merge --abort && git switch implement/<key> && git branch -m implement/<key>-issue-<n> implement/<key>-issue-<n>-old-$(git rev-parse --short implement/<key>-issue-<n>) && git switch -c implement/<key>-issue-<n> implement/<key>
```

## No push

At the end of the run, show that the run made no push and no pull request. Each command must print nothing. The second command is a GET. It compares the count of the pull requests with `<run>/pulls.txt`:

```sh
git for-each-ref 'refs/remotes/*/implement/<key>' 'refs/remotes/*/implement/<key>-*' | diff - "<run>/remotes.txt"
gh pr list --head 'implement/<key>' --state all --limit 1000 --json number --jq length | diff - "<run>/pulls.txt"
```

These commands show no push and no pull request. They show nothing about an opened issue, a closed issue, or a comment. Mark those three "unproven" in the report.
