---
name: test-audit
description: >-
  Audit the tests of a branch or a module: find the tests that protect nothing, check that each FR has a scenario and a test that can fail, and prove it with mutation testing. Use for "audit the tests before code-review", "find the tests that protect nothing", or "does each FR have a real test?".
---

# Test Audit

## Goal

Keep only the tests that can fail for a defect, and give each FR a test that can. Prove both with mutation testing, so that the user need not judge the tests.

A green suite, a coverage number, or a test name does not prove this: code can run with no check of its result. A mutant that a test kills proves that the test catches that defect.

## Steps

At each step that names a principle, apply it there. Follow the report rule of [principles](../principles/SKILL.md).

1. **Scope.** Read [audit.md](references/audit.md). Set the scope and its base. Make the run directory. For the whole test surface of one subsystem, also read [campaign.md](references/campaign.md).

   Apply [guard-one-way-doors](../principles/references/guard-one-way-doors.md). Here it saves `git status` and `git diff HEAD` before the first edit. Apply [sequence-verifiable-units](../principles/references/sequence-verifiable-units.md). Here it sets the start list: the failures of all checks by name, and the differences of the contract checks. This step is complete when `<run>/scope.txt` lists each test file and each production file in scope, and the files of the run directory exist.
2. **Map.** If the scope has a PRD, read [coverage.md](references/coverage.md). Map each FR in scope to its scenarios and its test code. Read each test in scope, as audit.md says.

   Apply [test-behavior-not-implementation](../principles/references/test-behavior-not-implementation.md). Here it judges each test: give it a provisional mark of the audit table and an evidence line. This step is complete when each FR in scope has a row in the coverage table, and each test declaration in scope has a mark.
3. **Mutate.** Read [mutation.md](references/mutation.md). Write the mutants in its order. Run them as it says. Apply guard-one-way-doors. Here it runs each mutant only in a copy in the run directory.

   Apply [prove-it-works](../principles/references/prove-it-works.md). Here it changes the proof of coverage: a test of the FR kills a mutant on its path. Confirm or change each mark. This step is complete when these conditions are true:
   - Each changed condition and each changed return value in scope has a mutant with a result. Or the report lists it as `not mutated`, as mutation.md allows.
   - Each mutant that survives is a gap or an equivalent mutant with evidence.
   - Each `delete` and `merge` outside the acceptance contract has its run without the suspects.
4. **Edit.** Outside the workflow, if the request only asks to find, review, or check, skip steps 4 and 5. Apply the marks, as audit.md says. Hold each change of the acceptance contract for the question of the stop rules.

   A mutant can survive on the path of an FR, or on a line that a `keep` test protects. For such a mutant, add one focused test. Put it at the seam of the module that owns the behavior, outside the acceptance contract. For a mutant that a held repair kills, follow mutation.md. Add no test for another reason. This step is complete when each mark is applied or held, and each added test kills its mutant.
5. **Check.** Run all checks. Compare the failures with the start list. Run the commands of "The checks" in audit.md. Run the contract check of [freeze.md](../bdd/references/freeze.md) for each issue with a freeze commit.

   If a contract check fails for a change of this audit without an approval, revert that change. Record it as a question in the report. This step is complete when these conditions are true:
   - No test that passed at the start fails.
   - Each absent test has its evidence.
   - Each contract check passes, or each difference has an approval or is in the start list.
6. **Report.** Write the report as [report.md](references/report.md) says. This step is complete when the report has each item of report.md.

## Completion conditions

- Each FR in scope has a scenario and a test that kills a mutant on its path. Or the report names the gap and its owner.
- Each deleted or merged test has the evidence of audit.md. Outside the acceptance contract, it also has its run without the suspects.
- Each change of the acceptance contract has the quoted approval of the user and its new freeze commits.
- All checks show no failure that the start list lacks.
- The working tree of the user holds each change of the user. No copy stays.

## Constraints

- **Product behavior.** Do not change the behavior of the product. Only when tests are its only callers, delete production code. Reproduce each product defect that a kept test shows. Report it with its owner: `tdd` for a known cause, `debug` for an unknown cause. In the workflow, the user starts the fix before `code-review`. A defect outside the scope is a draft follow-up issue.
- **Scenarios.** Write no scenario. A `Then` step whose text checks too little goes to `to-feature`, and so does an FR without a scenario.
- **Local work.** Commit on the branch of the scope. Do not push, open a pull request, or create an issue.

## Output

Write the report in Chinese, as report.md says. In the workflow, end with the next step: `code-review`.

## Stop rules

- Apply [no-stops-after-approval](../principles/references/no-stops-after-approval.md). Here it limits the questions to its stop reasons.
- A change of the acceptance contract is stop reason 1. Examples are the deletion of a scenario, and a change of a step definition or of a fixture of a scenario. Collect these changes. Do the other work first. At the end of step 4, ask once, with a numbered list and a recommended answer for each item. Do steps 5 and 6 while you wait.
- After the answer, commit each approved change and the test of each rejected repair. Follow audit.md and mutation.md. Then make the new freeze commits, as freeze.md says. Do steps 5 and 6 again. If nobody answers, keep the contract as it is. Record the questions in the report.
- Start no other skill.
