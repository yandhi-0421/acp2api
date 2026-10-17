---
name: bdd
description: >-
  Make the scenarios of one GitHub issue pass with BDD. The skill automates them, gets a trusted red, freezes the tests, and writes the product code. Use for "make the scenarios of issue 12 pass". Also use for "automate the scenarios of issue 12 and get a trusted red".
---

# BDD

## Goal

Make the Gherkin scenarios of one GitHub issue pass. Before you write the product code, prove that each test can fail.

The user confirmed the scenarios earlier. This skill freezes them. Do the whole issue in one run.

## Steps

At each step that names a principle, apply it there. Follow the report rule of [principles](../principles/SKILL.md).

1. **Read.** Read the issue, the scenarios with its slice tag, and the sections of the written design that it names. Read the section "Slice tags and run-control tags" of [gherkin.md](../to-feature/references/gherkin.md), and [document-map.md](../domain-modeling/references/document-map.md). Apply [guard-one-way-doors](../principles/references/guard-one-way-doors.md). Here it changes the start: save `git status` and `git diff` outside the repository. Keep each change of the user.

   If the current branch is the default branch, make or switch to the branch `issue-<n>`. Read [resume.md](references/resume.md). It finds the entry. This step is complete when you can name the scenarios with their FR tags, the entry, and the seams and test command, or their absence.
2. **Start.** Read [start.md](references/start.md). Apply [sequence-verifiable-units](../principles/references/sequence-verifiable-units.md). Here it sets the list that step 7 compares with. This step is complete when the list of failures and the output of the four FR coverage commands exist, and the units leave them unchanged.
3. **Automate.** Read [automate.md](references/automate.md). Automate all scenarios with the slice tag of the issue in one pass, and no other scenario. This step is complete when:

   - the dry run lists each scenario of the issue that is not blocked, with no undefined or ambiguous step;
   - the default run excludes `@wip`;
   - no product behavior changed;
   - the questions of automate.md have answers.
4. **Red.** Read [evidence.md](references/evidence.md). Run each scenario. Classify it as a trusted red, an existing green, or blocked. This step is complete when each scenario has one class, its command, and its output, and the four test questions have answers.
5. **Freeze.** List the contract files as [freeze.md](references/freeze.md) says, and each test file that may be in the contract. Scan them for a secret. Commit them as the freeze commit. It is the only record of the freeze, and the gate between the test part and the product part. Keep `@wip` on the scenarios, so that the default run stays green.

   Run the contract check for this issue, and for each done issue that the brief names. This step is complete when:

   - the freeze commit has no product code;
   - all checks, the default run included, match the list of step 2;
   - the contract checks pass.
6. **Build.** Read [build.md](references/build.md). Change product code, and no contract file. Use the skill `tdd` as the inner loop: one test at a time, at the seams of the written design.

   Apply [redesign-from-first-principles](../principles/references/redesign-from-first-principles.md). Here it adds a stop: if a requirement fits only as a bolt-on, read [stops.md](references/stops.md). This step is complete when each trusted red passes and the contract checks pass.
7. **Green.** Remove `@wip` from the tag lines of the scenarios of the issue that pass, and from no other line. Apply [prove-it-works](../principles/references/prove-it-works.md). Here it changes the claim: "done" rests on a run of the scenarios and of all checks, not on one suite that passes. Commit the removal, alone or with the last product commit. This step is complete when each item of "Completion conditions" holds, or the report names the blocked scenario or stop reason that excuses it.

## Completion conditions

- Each scenario of the issue passes in the default run without `@wip`, or the report names it as blocked.
- The default run has every scenario of the issue that is not blocked. None is skipped, `pending`, undefined, or ambiguous.
- All checks pass, except the failures from step 2, and no test is absent.
- The contract checks pass.
- The four FR coverage commands give the result of step 2.
- Each hunk of the commits since the start of step 1 maps to the issue.
- `git status --short` lists only the changes saved in step 1.

## Constraints

- **Scope.** The scope is the slice tag of the issue only, because each issue has its own check and its own freeze. Add no tag. Remove only `@wip`, in step 7. Change no scenario text, PRD, or written design.
- **Local work.** Commit on the current branch, with explicit paths. End the first line of each commit with `(#<n>)`, except the freeze commit. Do not push, merge, close an issue, or write a comment: each is a one-way door that `implement` owns.

## Output

Write the report in Chinese, with the Chinese terms of `GLOSSARY.md`. List:

- the issue, the branch, and each freeze commit of this issue, newest first;
- each scenario with its FR tags, its command, its class at step 4, and its result at the end;
- the failure line of each trusted red;
- the runner, the test command, and the seams, with each seam that is an agent decision;
- each step definition as new, reused, or changed, and each change of a dependency;
- the result of the contract checks, the diff of the listed files since the freeze commit, and each changed glossary entry;
- what the user or `implement` does next.

Add the report lines and the report items of the principles that you read. Without `implement`, ask once about the draft follow-up issues.

## Stop rules

Apply [no-stops-after-approval](../principles/references/no-stops-after-approval.md). Here it limits the questions to its stop reasons.

- If no scenario has the slice tag, or the issue is closed or a refactoring issue, stop.
- If a contract file must change after the freeze, read [stops.md](references/stops.md). Also read it for an absent runner or test dependency, a blocked scenario, or a run without a user.
- Start no later skill.
