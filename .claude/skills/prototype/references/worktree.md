# The worktree of a prototype

Sections: a new worktree, a branch that exists, uncommitted changes, no worktree.

Before you create the worktree, read "A branch that exists".

The worktree keeps the prototype out of the working tree of the user and out of each branch that a later merge reaches.

## A new worktree

With Bash, run this command:

```sh
git worktree add -b prototype/<name> <path> HEAD
```

- `<name>` is the question in a few lowercase words with hyphens, for example `checkout-layout`.
- `<path>` is an absolute path outside the working tree of the user, for example the folder `<repository>-prototype-<name>` next to the repository.
- Do not use the system temporary directory for `<path>`. The system can remove it while the user still needs the prototype.
- If the user gives a location outside the project, build in the worktree. Then copy the finished HTML file to that location.
- Do not move the session into the worktree. A tool that moves the session also changes the start commit and the directory of each later command.

## A branch that exists

Before you create the worktree, run `git fetch` if the repository has a remote. Then run `git branch -a --list '*prototype/*'` and `git worktree list`. If the branch `prototype/<name>` of the same question exists, locally or only on the remote, add to it. Do not make a second branch for the same question.

- If its worktree exists outside the working tree of the user, use it. If its worktree is the working tree of the user, ask the user.
- If `git worktree list` shows no worktree for the branch, run `git worktree add <path> prototype/<name>`. For a branch on the remote only, git makes a local branch that tracks it.
- If `git worktree list` marks its worktree "prunable", the folder is gone. Then add `-f`: `git worktree add -f <path> prototype/<name>`. Both commands keep the commits of the prototype.
- Never use `-B` on a branch that exists. It resets the branch, and the prototype is lost.

## Uncommitted changes

The worktree starts from the current commit, so it lacks the uncommitted changes of the user. If the target code has such changes, tell the user.

If the trial needs them, for example a new page that is not committed, copy them into the worktree. Do it as [guard-one-way-doors](../../principles/references/guard-one-way-doors.md) says for an isolated copy, without its comparison of `git status`. Copy only the untracked files that the trial needs.

The copied changes then go into the commit of the prototype. Nobody merges the branch, but the secret scan before the commit still applies. Step 2 of `SKILL.md` is not complete before this copy. If `git apply` fails, ask the user.

## No worktree

If you cannot create a worktree, use a new directory outside the project. A repository without a commit is such a case. Choose it as `<path>` above. This directory takes the place of the worktree in each step of `SKILL.md`. Say so in the report.

- Only a prototype that is one HTML file can go there: a logic prototype, or a UI prototype without an application. For another UI prototype, ask the user.
- Make no commit. Step 4 is then complete without the commit. In the report, give the directory in place of the worktree, the branch, and the commit.
