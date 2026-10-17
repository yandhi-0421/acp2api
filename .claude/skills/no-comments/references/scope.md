# The scope

Sections: the scope, the files that stay out, a large scope, a question, the run directory, the snapshot, the commits.

Run each command from the root of the repository.

## The scope

The scope is one of these:

- **The integration branch.** In the workflow, the skill runs once on the branch `implement/<key>`, after all issues of the PRD. The scope holds each file that the branch changed since its base. Uncommitted work of the user stays out.
- **A named scope.** The user names paths, a module, or a diff. A named path or module has no base, so the subagent judges each comment in it. A named diff has the base of the diff.
- **No name.** The current branch against the default branch, with the uncommitted changes and the untracked files. On the default branch, the scope is the uncommitted changes and the untracked files, with `HEAD` as the base.

For the integration branch and for no name, the base is the merge base of the default branch and `HEAD`. The first command prints the default branch with the prefix `origin/`. If `origin/HEAD` is not set, run the second command, which is a GET. If `origin/<name>` exists, use it. Else use `<name>`.

```sh
git symbolic-ref --short refs/remotes/origin/HEAD
gh api repos/{owner}/{repo} --jq .default_branch
git merge-base <default branch> HEAD
```

These commands list the files. The option `core.quotePath=false` keeps each path that has other than ASCII characters readable.

```sh
git -c core.quotePath=false diff --name-only <base> HEAD
git -c core.quotePath=false diff --name-only <base>
git -c core.quotePath=false ls-files --others --exclude-standard -- <path>
git -c core.quotePath=false ls-files -- <path>
```

- **The integration branch:** the first command.
- **No name:** the second command, and the third command without `-- <path>`.
- **A named path or module:** the last two commands.
- **A named diff:** the command of the diff, with `--name-only`.

## The files that stay out

Remove these files from the scope. Count them by kind for the report.

- Each feature file. A feature file is a document of the plan, and `to-feature` owns it.
- Each file of the acceptance contract. The command below lists each file that a freeze commit that `HEAD` can reach lists. A deleted line in such a file fails the contract check of [freeze.md](../../bdd/references/freeze.md), and a comment is no reason to change the acceptance contract.
- If the project has feature files but no freeze commit: each file that the configuration of the Gherkin runner loads. Examples are the step definitions, the hooks, and the support files. Also each fixture, test data file, and fake that a step definition reads.
- Generated files, vendored files, lock files, binary files, and documents such as Markdown files.
- Each path that the working tree does not hold, for example a file that the branch deleted.
- On the integration branch, each file with an uncommitted change of the user. `git -c core.quotePath=false diff --name-only HEAD` and `git -c core.quotePath=false diff --name-only --cached` list these files.

```sh
git log HEAD --format=%B --grep='^Freeze the acceptance contract of #' | awk '/^Contract files:$/{m=1;next} /^$/{m=0} m' | sort -u
```

## A large scope

A large scope has more than about 40 files or 3,000 changed lines. One subagent cannot read it well. Split it by directory into two to four parts of about equal size. Write the list of each part into `<run>/scope-<n>.txt`. Give each part to its own subagent, with the path of its list as the path of `scope.txt`. Start the subagents at the same time.

## A question

If the request only asks if a comment is needed, the run judges, but it edits nothing:

- The scope is the file of that comment, without a base. Give the subagent the comment too.
- Edit no file. Tell the subagent to edit nothing.
- Skip the start list and steps 4 and 5. Make the snapshots, so that you can find an edit of the subagent.
- The files that stay out do not apply, because the run edits nothing.
- In "The rules" of [verify.md](verify.md), a judgment takes the place of each edit. In the completion conditions, a judgment takes the place of each deletion and each keep.
- In step 6, commit nothing. Send the report only in the chat, because `code-review` reads the file of the report.

## The run directory

The report goes into `<report dir>`. In the workflow, `<report dir>` is the run directory of `implement`. "The run directory" in [commands.md](../../implement/references/commands.md) of `implement` gives its path. Then `code-review` finds the reports in one place. On another branch, `<report dir>` is `${TMPDIR:-/tmp}/no-comments-<repo>-<branch>`, with each `/` of the branch name as `-`. `<repo>` is the name of the root directory of the repository.

On the default branch, the run can need a branch for its commits. It needs one when it edits files and no file in scope holds an uncommitted change of the user. Then first make the branch `no-comments/<scope>`. `<scope>` is a short name of the scope. Then the report and the commits are on one branch.

The work files go into `<run>`, which is `<report dir>/no-comments`. The command below makes `<run>` and prints it:

```sh
b=$(git branch --show-current); [ -n "$b" ] || b=$(git rev-parse --short HEAD); r=$(basename "$(git rev-parse --show-toplevel)"); case "$b" in implement/*) d="${TMPDIR:-/tmp}/implement-$r-${b#implement/}" ;; *) d="${TMPDIR:-/tmp}/no-comments-$r-$(printf '%s' "$b" | tr '/' '-')" ;; esac; mkdir -p "$d/no-comments" && printf '%s\n' "$d/no-comments"
```

At the start, write these files into `<run>`:

- `status.txt` and `diff.txt`: the output of `git status` and `git diff HEAD`;
- `head.txt`: the output of `git rev-parse HEAD`;
- `scope.txt`: each file in scope, one path on each line, relative to the root;
- `start.txt`: the command and the count of tests of each check, and each failure by name.

Find the commands of the checks in `CLAUDE.md` or in the CI configuration.

## The snapshot

The subagent edits the working tree, and the working tree can hold uncommitted work of the user. Two snapshots show the edits of the run apart from that work. A snapshot is a git tree object of the whole working tree, made with a copy of the index in `<run>`. The commands change no file of the user and not the index of the user.

Make the first snapshot after the start list, before the subagent starts:

```sh
cp "$(git rev-parse --git-path index)" "<run>/user.index" && cp "<run>/user.index" "<run>/before.index" && GIT_INDEX_FILE="<run>/before.index" git add -A && GIT_INDEX_FILE="<run>/before.index" git write-tree > "<run>/before.tree"
```

After the subagent returns, make the second snapshot with this command. Keep `user.index` unchanged.

```sh
cp "$(git rev-parse --git-path index)" "<run>/after.index" && GIT_INDEX_FILE="<run>/after.index" git add -A && GIT_INDEX_FILE="<run>/after.index" git write-tree > "<run>/after.tree"
```

## The commits

- **The integration branch.** Commit on it. One commit holds the deletions and the restores. Its body lists each accepted reshape flag: the symbol, the change, and the owner. A second commit holds the refactorings of [verify.md](verify.md), if any. Its body names each flag that it closes.
- **A committed scope on another branch.** Commit there, in the same way. On the default branch, "The run directory" above makes the branch first.
- **A scope with uncommitted work of the user.** If a file in scope holds an uncommitted change of the user, do not commit. The files hold the work of the user. Say so in the report.

Commit with explicit paths, so that a staged change of the user stays out of the commit: `git commit -m <message> -- <paths>`.
