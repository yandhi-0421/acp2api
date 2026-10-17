---
name: tdd
description: >-
  Write code test-first: one test at a time, and each test fails before its code exists. Use for "do this test-first", "write a failing test first", "red-green", or "add a regression test".
---

# TDD

## Goal

Build each behavior with the red-green loop, one test at a time.

A test that you write after the code often restates the code, so it cannot fail for a defect. A test that failed first, for the intended reason, proves that it can catch the defect.

The skill `bdd` reads this skill as its inner loop. The skill `debug` reads it for the regression test of a bug. Alone, it serves a direct request.

## When the loop applies

When a cheap local test, or a scenario of `bdd`, can reach the behavior, use the loop. If no practical test exists, do not force one. Apply [test-behavior-not-implementation](../principles/references/test-behavior-not-implementation.md). Here it replaces the test with the closest executable check.

For a bug, you must know the cause. Alone, if the user or the issue does not state it, use the skill `debug`. It finds the cause, and then it reads this skill.

## Steps

At each step that names a principle, apply it there. Follow the report rule of [principles](../principles/SKILL.md).

1. **Seams.** Before the first test, list the seams under test in the chat. Take them from the section "Seams" of the written design. The user approved them there, so ask nothing about them. You cannot test everything, and these seams put the tests on the critical paths and the complex rules.

   If no written design names a seam for the behavior, apply test-behavior-not-implementation. Here it chooses the seam, as its section "Where to test" says. Report each such seam as an agent decision. If the shape of an interface is in question, apply [deep-modules](../principles/references/deep-modules.md). Here it gives the words for the decision and the report. A test that needs a change of the designed interface is stop reason 4.

   If no start list exists from `bdd` or `debug`, apply [sequence-verifiable-units](../principles/references/sequence-verifiable-units.md). Here it sets the start: run all checks. Record each failure by name. This step is complete when each seam has its source and the start list exists.
2. **Red.** If a `bdd` scenario or a test in the suite fails for the behavior, use it as the red. If not, write one test at a seam of step 1. Apply test-behavior-not-implementation. Here it sets the source of the expected value. For a bug, this test is the regression test, and it must fail for the cause of the reported symptom.

   Run the test. Question 2 of the self-check of the same principle decides what counts as red. If the test passes before the change, answer that question.

   If the test fails before its assertion because a seam does not exist yet, add the seam with no behavior. The seam returns the empty value that the test reads, and it does not throw. Then run the test again.

   This step is complete when you have the command of the test and the failure line that counts as red.
3. **Green.** Write only enough code to make this test pass. Apply [subtract-before-you-add](../principles/references/subtract-before-you-add.md). Here it limits the code: no code and no test for a later need. For a bug, apply [fix-root-causes](../principles/references/fix-root-causes.md). Here it adds each other instance of the defect, each in a cycle of its own.

   Run the test and the other tests of the changed modules. This step is complete when this test passes, and the other tests show no failure that the start list lacks.
4. **Repeat.** Take the next test or table row, one at a time: a batch checks a shape that you imagined. Do not refactor inside a cycle. Apply sequence-verifiable-units. Here it makes each refactoring a unit of its own, or a draft follow-up issue. This step is complete when each item of "Completion conditions" holds.

## Completion conditions

- Each behavior in the scope has a test with a red run of step 2. It failed before its code existed, or against a planted regression.
- Without a practical test, the behavior has its closest check, and the report gives the reason.
- Each test of the loop passes. All checks show no failure that the start list lacks.
- No cycle holds a refactoring, or code or a test for a later need.
- Each seam comes from the written design, or the report names it as an agent decision.

## Constraints

- **Commits.** Apply sequence-verifiable-units. Here it sets the order of the commits: for a bug fix, the red test after step 2, and the fix after step 3. For a cycle of a feature, commit the test and its code together after step 3. The commit rules of the skill that called this skill win. Alone, only when the user asks, or on an issue branch, commit.
- **Test runner.** Use the test runner of the project, or of the standard library of its language. If neither exists, apply [choose-boring-technology](../principles/references/choose-boring-technology.md). Here it gives the facts for a recommended runner. Then stop with stop reason 4. If `debug` called this skill, you can use the closest executable check instead of the stop.

## Output

If `bdd` or `debug` called this skill, give these items to its report. Alone, write the report in Chinese, with the Chinese terms of `GLOSSARY.md`. List:

- each test with its seam, the failure line of its red, and the run that passes after it;
- for a behavior without a practical test, the reason, and the closest check with its command and output.

Add the report lines and the report items of the principles that you read.

## Stop rules

- Apply [no-stops-after-approval](../principles/references/no-stops-after-approval.md). Here it limits the questions to its stop reasons.
- Start no later skill, except `debug` as "When the loop applies" says.
