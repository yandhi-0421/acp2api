# The commands

Sections: the scope alone, the history, the pitfalls, what a history hides.

## The scope alone

Alone, take the diff of the branch against its merge base with the target branch. If the request names no target branch, use the default branch, and say so. For uncommitted work, take `git diff HEAD` and each untracked file that git does not ignore. For the staged changes only, take `git diff --cached`, also for the copy. Record the base, the head, and the paths in the report.

For a named pull request, run `gh pr view '<number>' --json baseRefName,baseRefOid,headRefOid,isCrossRepository`. Fetch the head and the target branch with `git fetch origin 'pull/<number>/head' '<baseRefName>'`. Then `<head>` is the `headRefOid`, and `<target>` is the `baseRefOid`. A local branch can be behind, and after a merge the target branch holds the head. Do not check out the head. If `isCrossRepository` is true, read "A pull request from a fork" in proof.md.

For a planned change without a diff, treat the named symbols, fields, or dependency as the change, and say so. Make the proof in the isolated copy of [proof.md](proof.md).

## The history

First read "What a history hides" below. Run each command from the root of the repository. Each command only reads. Replace each placeholder in angle brackets. If GitHub is not available, use the local git history, and say so in the report.

Resolve the revisions, and see the changed paths, the size, and the commits. `<target>` is the target branch, and `<base>` is the output of `git merge-base` or the base that `code-review` gives. For uncommitted work, `<base>` and `<head>` are `HEAD`. Then omit `"<head>"` from `git diff` and `git grep`, so that they read the working tree. For the staged changes only, also add `--cached` to them. Over merge commits, add `--first-parent` to `git log`.

```sh
git rev-parse --verify "<ref>^{commit}"
git merge-base "<target>" "<head>"
git diff --name-status -M "<base>" "<head>"
git diff --shortstat "<base>" "<head>"
git log --oneline "<base>..<head>"
```

See the changed hunks of a path. The text after the second `@@` of each hunk line often names the function or the class.

```sh
git diff -U0 "<base>" "<head>" -- '<path>'
```

Find the users of a symbol at the head, outside the changed path. For uncommitted work, add `--untracked`, but not together with `--cached`:

```sh
git grep -n -w -F '<symbol>' "<head>" -- . ':(exclude)<changed path>'
```

Find when a text came in or went out. Search with no path, because a moved file hides the first commit of its text. Use the shortest text that names the code.

```sh
git log -S'<text>' --reverse --oneline --name-status "<head>"
git log -G'<regex>' --oneline "<head>"
```

Find the history of some lines, and the renames of a file. For lines that the change deletes, use `"<base>"` in place of `"<head>"`:

```sh
git log -L '<start>,<end>:<path>' --oneline "<head>"
git blame -L '<start>,<end>' "<head>" -- '<path>'
git log --follow --oneline "<head>" -- '<path>'
```

Find the files that change together with a path. They often hold the downstream users that a symbol search misses.

```sh
git log --format= --name-only --full-diff -20 "<head>" -- '<path>' | sort | uniq -c | sort -rn
```

Find the pull request of a commit. Its message holds `(#1234)` or "Merge pull request #1234". For a squash commit, ask GitHub.

```sh
git log -1 --format=%B "<commit>"
gh api 'repos/{owner}/{repo}/commits/<commit>/pulls' --jq '.[].number'
```

Read the pull request and the issues that it links:

```sh
gh pr view "<number>" --json title,body,author,createdAt,mergedAt,labels,closingIssuesReferences,comments,reviews,files
gh pr diff "<number>" --name-only
gh issue view "<number>" --json title,body,comments
```

## The pitfalls

- A path or a text from the diff can hold shell code, for example `$(...)`. Thus the templates put it in single quotes. If it holds a single quote, use a shorter part of it.
- Quote each revision. In zsh with `extendedglob`, an unquoted `HEAD^` fails with "no matches found".
- Use `git grep`, not `rg`. `rg` is not always installed.
- Without `--full-diff`, `git log --name-only -- <path>` lists only that path.
- `--follow` together with `--reverse` prints only a part of the history, with no error. Do not use them together.
- `{owner}/{repo}` works only in a clone with a GitHub remote. Elsewhere, give the name of the repository. For a commit that is not on GitHub, the API gives an error.

## What a history hides

- A squash merge removes the commits of the branch. Read the text and the discussion of the pull request.
- A message such as "small refactor" can hide a change of behavior. Read the diff, not the message.
- A copied pattern can carry the same break to another location. Search for the other copies.
- A commit of a bot, for example an update of a dependency, rarely gives a reason.
- Defensive code is often the fix of an incident. Look for "fix for incident", "add defensive check", and a revert that a new apply follows.
- The code is not evidence of its own intent. The name of a function does not say why the function exists.

Examples of guards are a null check, a retry, a timeout, a rate limit, a feature flag, and a catch block. The last commit of a guard is often not its reason. For each deleted or weakened guard, run `git log -S'<text>' --reverse --oneline --name-status "<head>"`, with no path. If the file of the guard holds the text twice or more at the base, use `git log -L` at `"<base>"` instead. Take the first commit that adds the text to the file of the guard or to its old path. If that commit or its pull request names a bug or an incident, cover that case in a safety fact.
