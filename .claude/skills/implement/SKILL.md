---
name: implement
description: >-
  Implement the open issues of one PRD or one written design, each on its own branch by a fresh subagent. Merge each issue that passes all checks into one integration branch, one at a time. Use for "implement these issues", "build this PRD", "implement issue #12".
---

# Implement

## Goal

Build the sub-issues of one parent issue on one integration branch. Work one issue at a time. Give each issue to a fresh subagent that gets pointers only. Only after your own run of the checks passes, merge the issue.

The key is the title of the parent issue. The integration branch, `implement/<key>`, holds the merged work. An issue branch is `implement/<key>-issue-<n>`, and it holds the work of one issue until the merge.

## Steps

At each step that names a principle, apply it there. Follow the report rule of [principles](../principles/SKILL.md).

1. **Read.** Find the key, the parent issue, and the improve mode as [requests.md](references/requests.md) says. Read `docs/agents/issue-tracker.md`, `GLOSSARY.md`, the PRD with its status, and the written design. Read the sub-issues, their labels, and their blocking edges, as [commands.md](references/commands.md) says. If an integration branch, an issue branch, or a run directory of this key exists, read [rerun.md](references/rerun.md).

   Apply [make-operations-idempotent](../principles/references/make-operations-idempotent.md). Here it changes what finds an earlier run: the key, the merge commits, and the issue branches. An issue is done when a merge commit on the integration branch names it and no scenario of its slice tag has `@wip`. Thus `@wip` that returns reopens the issue. The frontier is the set of sub-issues that "The frontier" in commands.md lists.

   Apply [no-stops-after-approval](../principles/references/no-stops-after-approval.md). Here it changes the start: show the plan, and go on without a wait. The plan names the branch, the order of the frontier by issue number, and the run mode. This step is complete when you showed the plan, and gave a reason for each issue that you leave out.
2. **Prepare.** Work in the working tree of the user. Save `git status` and `git diff HEAD` in the run directory of commands.md. Keep each change of the user. Make the integration branch from the current commit. Commit the plan documents of commands.md on it, with explicit paths. Do not rebase it, because a rebase removes the merge commits, and they are the record of the done issues.

   Apply [sequence-verifiable-units](../principles/references/sequence-verifiable-units.md). Here it sets the start state. Run all checks. Find their commands in `CLAUDE.md` or the CI configuration. If `<run>/start.txt` is absent, write the command, the test count, and the failure names of each check into it. This step is complete when the integration branch has a clean working tree, and the start file exists.
3. **Hand over.** Take the lowest-numbered issue of the frontier. Make or resume its issue branch.

   Read [brief.md](references/brief.md). Write the brief. Start one fresh subagent. Tell it only the path of its brief. The brief names the recipe: `bdd` for a slice issue, and [refactoring.md](references/refactoring.md) for a refactoring issue. Only when the user asks for parallel work, read [parallel.md](references/parallel.md).

   Apply [build-the-lever](../principles/references/build-the-lever.md). Here it changes the hand-over: the brief is one file outside the repository. If one script can do every unit of the issue, run the script yourself. This step is complete when the subagent has returned its report, or the script ran on the issue branch and committed.
4. **Check and merge.** Apply [prove-it-works](../principles/references/prove-it-works.md). Here it changes who proves. The report of the subagent is a lead, so run each merge check of [check.md](references/check.md) yourself.

   If each merge check passes, merge the issue with `--no-ff`, as commands.md says. Read the merge back. If a merge check fails, check.md gives one retry. If a merge check fails again, the issue fails. This step is complete when each merge check has its output, and you merged the issue or marked it failed. Then, until the frontier is empty, repeat steps 3 and 4.
5. **Finish.** Run all checks on the integration branch. Write the report. This step is complete when the run of all checks has its output, and you wrote the report.

## Completion conditions

- The run merged each sub-issue, or the report names it with its reason.
- All checks on the integration branch show no failure that `start.txt` lacks.
- The run made no push, pull request, opened issue, closed issue, or comment.

## Constraints

- **Local only.** Apply [guard-one-way-doors](../principles/references/guard-one-way-doors.md). Here it limits the run to local commits. The approval of the plan does not confirm a push, a pull request, an opened issue, a closed issue, or a comment. Before you run `git reset --hard`, `git clean -fd`, or `git stash drop`, make a backup.
- **Scope.** An issue holds the scenarios with its slice tag. Do not make the scope smaller in silence. A done issue stays done, and a change to its acceptance contract is stop reason 1.

## Output

Write the report in Chinese, with the items of [output.md](references/output.md).

End with the next steps: `no-comments`, `test-audit`, `code-review`, and `pr`. Ask once if you may create the draft follow-up issues.

## Stop rules

- If `docs/agents/issue-tracker.md` does not exist, stop. Tell the user to run `/workflow-setup`.
- If no parent issue or no sub-issue exists, stop. Suggest `to-tickets`.
- If the PRD has no written design, do not stop for it alone. Say so in the plan.
- If an uncommitted change is not a plan document and rerun.md does not adopt it, ask the user. Name stop reason 3.
- If a sign of a wrong design appears, read [wrong-design.md](references/wrong-design.md).
- Apart from these rules, only when a stop reason of no-stops-after-approval applies, stop. Name the reason in the question.
- Start no later skill.
