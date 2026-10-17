# The entry

This file is the depth of step 1. The full run goes through steps 1 to 7 in order. This file finds the entry of this run.

The brief is the file that `implement` gives to a subagent. A run without `implement` has no brief. Then check only this issue.

Count a scenario with the slice tag of an open issue as confirmed. Match the slice tag as a whole word, so that `@issue-1` does not match `@issue-12`.

If the request names several issues (the request, not the field "Done issues" of a brief), stop and suggest `implement`. If it names an FR tag or a scenario, take the issue from its slice tag. If the slice tags differ, treat the request as several issues.

The constraint "Local work" of `SKILL.md` gives the first line of each commit. A commit of this issue is one of these:

- a commit of the branch that ends its first line with `(#<n>)`;
- the commit "Save the unfinished work of issue #<n>" of `implement`.

An uncommitted file is earlier work of this issue in four cases:

- an earlier commit of this issue changed the file;
- the file holds a step definition of a scenario of this issue;
- the file is a fixture, test data, fake, hook, or runner configuration, and an adopted file holds a step definition;
- the file is a manifest or a lock file, and its diff adds a test dependency that an adopted file uses.

Adopt such a file. Say so in the report. Treat each other uncommitted file as a change of the user. Keep it out of each commit. Do not map its hunks to the issue.

Find the freeze commit of this issue, as the section "Find the freeze commit" of `freeze.md` says. Then choose the entry. Use the first line that matches:

- A freeze commit exists: see "A freeze commit exists".
- No freeze commit, and a scenario of this issue has no `@wip`: see "No freeze commit, but a scenario without `@wip`".
- No freeze commit, and earlier work of this issue exists: see "A rerun before the freeze".
- None of these: go to step 2.

If the user asks to automate only, see "Automate only", whatever the entry.

## The list of failures on a rerun

Use the start file of the brief. If the brief names none, run all checks in an isolated copy. Use the commit where the branch of this issue starts. It is the merge base with the branch that it was cut from. Keep the output of the four FR coverage commands from that copy, as `start.md` says.

A new isolated copy, for example a worktree, lacks the ignored files. Examples are the installed dependencies and the `.env` file. Make the dependencies with the commands of the project. Copy the `.env` file. If every test fails in the copy, repair the copy before you record the list.

## A freeze commit exists

Do not freeze again, except as items 2 to 4 say. Take the list of failures as "The list of failures on a rerun" says.

1. Run the contract check for this issue, and for each done issue that the brief names.
2. If they pass, run the scenarios of this issue with the command of "The run" in `evidence.md`. Then act on each scenario that fails:
   - It has `@wip` and undefined steps. It was not automated. Go to step 3 for it, and on through step 5.
   - A product commit of this issue follows the freeze commit. The failure is the work of step 6. A fault in a contract file is stop reason 1.
   - In each other case, it must be a trusted red, as `evidence.md` defines it.

   If no scenario took the first case above, make no new freeze commit. Go to step 6.
3. `to-feature` reopens a scenario with the approval of the user. A reopened scenario is one of these:
   - it has `@wip` again after a commit removed it since the freeze commit;
   - it has an undefined step, and no commit of this issue changed its text since the freeze commit.

   If the check of this issue fails only in reopened scenarios, redo steps 3 to 5 for them. Make the new freeze commit as the section "The freeze commit" of `freeze.md` says.
4. Any other failure is stop reason 1. If the brief or the user in the chat already approved the change, follow the way back in `stops.md`.

## No freeze commit, but a scenario without `@wip`

You cannot tell if the scenarios of this issue are frozen. An absent record does not show that they are not frozen. Treat them as frozen: a change to their test files is stop reason 1. Take the list of failures as "The list of failures on a rerun" says. Run the scenarios. Go to step 6.

## A rerun before the freeze

Earlier work of this issue exists when the branch has a commit of this issue, or when you adopt an uncommitted file.

Take the list of failures as "The list of failures on a rerun" says. Do each unit of step 2 that no commit of this issue shows. Then go to step 3.

## A rerun

Apply [make-operations-idempotent](../../principles/references/make-operations-idempotent.md). Here it sets the end state of a rerun to the end state of one clean run. It covers three operations: the exclusion rule, the removal of `@wip`, and the freeze commit. A second run of each operation leaves the state of the first run.

## Automate only

Only when the user asks to automate only, stop after step 5. If a freeze commit exists, do items 1 to 4 of "A freeze commit exists". Stop before step 6. Except for an empty seam of `start.md`, write no product code. The report states that the product part did not run.
