# The commands

Sections: before the question, push and create, replace the text, the read-back, the recovery.

Run each command from the root of the repository. `<branch>` is the current branch. `<base branch>` and `<base>` are as [inputs.md](inputs.md) defines them. `<n>` is the number of the pull request.

## Before the question

Scan each commit that the push publishes, the title file, and the body file. Use the secret scanner of the project, or the patterns of [protect-secrets-and-personal-data](../../principles/references/protect-secrets-and-personal-data.md). Scan the output of these commands. The option `--cc` also shows the changes inside a merge commit.

```sh
git log -p --cc --format=%H <base>..HEAD
cat "<run>/title.txt" "<run>/body.md"
```

Then show the user these items:

- the full title, as it is in `<run>/title.txt`, and the full body, as it is in `<run>/body.md`;
- for an open pull request, its current title, base, and body, because the replacement overwrites them;
- the base branch and the head. If the base branch is not the default branch, say that GitHub does not close the issues at the merge;
- the commits and the changed files of the output of the first two commands below;
- for an open pull request, the commits that the push adds, from the third command;
- the uncommitted files that the pull request leaves out, from the last command;
- the result of the secret scan;
- the commands of "Push and create" or "Replace the text" that you will run;
- the recovery plan of "The recovery".

```sh
git log --oneline <base>..HEAD
git diff --stat <base>...HEAD
git log --oneline origin/<branch>..HEAD
git status --short
```

Wait for a clear yes.

## Push and create

```sh
git push -u origin <branch>
gh pr create --base <base branch> --head <branch> --title "$(cat "<run>/title.txt")" --body-file "<run>/body.md"
```

If the push fails, follow the stop rules of `SKILL.md`.

## Replace the text

For an open pull request of the branch, first save its title and body. Then push. Then replace the title and the body:

```sh
gh pr view <n> --json title --template '{{.title}}' > "<run>/old-title.txt"
gh pr view <n> --json body --template '{{.body}}' > "<run>/old-body.md"
git push origin <branch>
gh pr edit <n> --title "$(cat "<run>/title.txt")" --body-file "<run>/body.md"
```

## The read-back

```sh
gh pr view <n> --json number,url,title,baseRefName,headRefName,closingIssuesReferences
gh pr view <n> --json body --template '{{.body}}' > "<run>/readback.md"
diff "<run>/body.md" "<run>/readback.md"
```

The `diff` command must print nothing. Compare the title, the base, and the head with the approved values. Compare `closingIssuesReferences` with the issue lines. GitHub lists them only when the base branch is the default branch. If a value differs, report it as a failure of step 4.

## The recovery

A push and a pull request cannot be undone completely. The notifications that GitHub sent stay. These steps limit the effects:

- For a new pull request: `gh pr close <n>`. Only when the push made the remote branch, delete it with `git push origin --delete <branch>`.
- For a pull request that existed before: do not close it. Do not delete its branch. Restore its old title and body with the command below.

```sh
gh pr edit <n> --title "$(cat "<run>/old-title.txt")" --body-file "<run>/old-body.md"
```

Each of these steps is a one-way action too. Ask the user before you run it.
