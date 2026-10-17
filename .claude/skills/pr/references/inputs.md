# The inputs

Sections: the branch, the reports, the gate, the base and the title, the door and the blast radius, the issues to close.

Run each command from the root of the repository. The run is in the workflow when the current branch is `implement/<key>`. A branch `implement/<key>-issue-<n>` is an issue branch of `implement`. For an issue branch, stop. Name its integration branch.

## The branch

- The current branch must be a branch, and not the default branch. Else stop. Tell the user.
- An open pull request of the branch can exist. This command is a GET: `gh pr list --head <branch> --state open --json number,url,title,body,baseRefName`.
- Fetch the branch with `LC_ALL=C git fetch origin <branch>`. If the fetch fails with `couldn't find remote ref`, the remote branch does not exist yet. Then skip the next check. For another failure, stop. Report it.
- If `origin/<branch>` exists, `git rev-list --count HEAD..origin/<branch>` must print `0`. Else the stop rules of `SKILL.md` apply.
- `git status --short` lists the uncommitted files. They are not part of the pull request. The question of step 3 names them.

## The reports

In the workflow, `<report dir>` is the run directory of `implement`. "The run directory" in [commands.md](../../implement/references/commands.md) of `implement` gives its path. There, `report.md` is the report of `implement`, and `start.txt` is its start file. `code-review.md` holds the reports of `code-review`. On another branch, the reports of `code-review` are in `${TMPDIR:-/tmp}/code-review-<repo>-<branch>/code-review.md`, with each `/` of the branch name as `-`. `<repo>` is the name of the root directory of the repository.

The work files go into `<run>`. In the workflow, `<run>` is `<report dir>/pr`. On another branch, it is `${TMPDIR:-/tmp}/pr-<repo>-<branch>`. The command below makes `<run>` and prints it:

```sh
b=$(git branch --show-current); r=$(basename "$(git rev-parse --show-toplevel)"); case "$b" in implement/*) d="${TMPDIR:-/tmp}/implement-$r-${b#implement/}/pr" ;; *) d="${TMPDIR:-/tmp}/pr-$r-$(printf '%s' "$b" | tr '/' '-')" ;; esac; mkdir -p "$d" && printf '%s\n' "$d"
```

The file `code-review.md` can hold several reports, the newest one last. The first line of each report is `Head: <hash>`, the head that the review saw. A report with ` with uncommitted changes` in its head line does not name the head. The header of each report gives the line `Recommendation:` with the merge decision.

## The gate

In the workflow, take the newest report of `code-review.md`. Each of these must be true:

- The file exists.
- The head of the report is the output of `git rev-parse HEAD`. Else commits came after the review.
- The head line does not end with ` with uncommitted changes`. Such a review did not see the commits of the branch.
- The merge decision is `MERGE`.

If one is false, stop. Say which condition is false. Exception: after you named the decision, the user still asks for this pull request. Then go on. If the user asks only for text, also go on. State the decision in the line **Review** of the body.

Outside the workflow, no gate applies. If a report of `code-review` names the head, use it. Else the line **Review** says that no `code-review` ran at the head. The report of `pr` then recommends `code-review` before the merge.

## The base and the title

- **The base branch.** For an open pull request, its `baseRefName`. Else, in the workflow, the default branch. The base in the report of `implement` is a commit on it. Outside the workflow, the target branch of the report of `code-review`, or else the default branch. `<base branch>` is the name of the branch without `origin/`.
- **The default branch.** `git symbolic-ref --short refs/remotes/origin/HEAD` prints it with the prefix `origin/`. If that command fails, use `gh api repos/{owner}/{repo} --jq .default_branch`.
- **The base.** `<base>` is the output of `git merge-base origin/<base branch> HEAD`. If `origin/<base branch>` does not exist, use `<base branch>`.
- **The title.** In the workflow, `<key>`. Else a short statement of the change in the imperative, with at most 72 characters. Only when the user asks for a new title, replace the title of an open pull request.

## The door and the blast radius

Use each report of `code-review.md` that covers commits of this branch. For a report, the first command must succeed, and the second command must fail:

```sh
git merge-base --is-ancestor <its head> HEAD
git merge-base --is-ancestor <its head> <base>
```

The second review after the fixes covers only the diff of the fixes. Thus one report alone can be too narrow:

- The door is one-way if one of these reports gives a one-way door.
- Order the words of the blast radius as `local`, `internal`, `not measured`, `external`. Take the last word in this order that one of these reports gives. `not measured` comes after `internal`, because nobody measured how far the change reaches.

If no report names the head, the reports do not cover the newest commits. Then also answer self-check 2 of [guard-one-way-doors](../../principles/references/guard-one-way-doors.md) for the door. Add the word `not measured` to the words of the reports.

## The issues to close

In the workflow, take the list of the report of `implement`: the issues that the final pull request closes. Then run the command below. Remove each issue that it prints as `reopened`. A skill can reopen an issue after `implement`, with `@wip` on a scenario. If you remove an issue, also remove the parent issue.

If the report of `implement` is absent, the merge commits of the branch give the done issues. Step 1 of [implement](../../implement/SKILL.md) defines a done issue. The command prints `done` or `reopened` and the number of each merged issue:

```sh
for n in $(git log --first-parent --format=%s <base>..HEAD | grep -oE '^Merge issue #[0-9]+' | grep -oE '[0-9]+' | sort -un); do
  if git grep -nE "@issue-$n([[:space:]]|\$)" -- '*.feature' | grep -qE '@wip([[:space:]]|$)'; then printf 'reopened\t%s\n' "$n"; else printf 'done\t%s\n' "$n"; fi
done
```

Without the report of `implement`, the parent issue is not known. Write no line for it, and say so in the report of `pr`.

Outside the workflow, close only the issues that the user names. In an open pull request, each issue that a keyword such as `Closes` or `Fixes` names counts as an issue that the user names.
