---
name: debug
description: >-
  Find the cause of a bug with a reproduction check first. Then fix the bug, or only report the cause. Use for "fix this bug", "why does this fail", "this test is flaky", or "this got slow".
---

# Debug

## Goal

Find the first wrong state that explains the reported symptom. Fix the defect in the layer that owns the behavior. Prove the fix with the reported case.

## Scope

- A request for the cause only, for example "why does this fail", asks for a diagnosis. It allows only inspection and safe local checks. Make each file change in an isolated copy. End after step 4 with `Diagnosed` and the smallest repair.
- Any other request asks for a fix. Change only what the bug needs.
- The skill applies to each bug. For a simple bug, you can skip step 3 or 4, but give the reason in the report.

## Steps

At each step that names a principle, apply it there. Follow the report rule of [principles](../principles/SKILL.md).

1. **Define the failure.** Record the expected result, the actual result, the input, the environment, and the state. Keep the exact reported symptom. Apply test-behavior-not-implementation. Here it sets the source of the expected result, and a good state also counts. If no source states it, it is an open decision for `grilling` and `to-prd` (stop reason 4). Read each section of [kinds.md](references/kinds.md) that fits this bug.

   Apply [sequence-verifiable-units](../principles/references/sequence-verifiable-units.md). Here it sets the start: run all checks. Record each failure by name. This step is complete when the record and the start list exist.
2. **Reproduce.** Apply [fix-root-causes](../principles/references/fix-root-causes.md). Here it sets the first move: before any hypothesis, make a reproduction check. Be relentless in this step, because a check that fails on this bug is most of the diagnosis. Try the ways of [techniques.md](references/techniques.md) in order. Then improve it, as techniques.md says.

   Run the check several times. Capture the exact symptom. Then read kinds.md again.

   Apply [protect-secrets-and-personal-data](../principles/references/protect-secrets-and-personal-data.md). Here it redacts each command, output, artifact, and fixture.

   This step is complete when you have the command and the failure line of the reported symptom, with the rate for an unstable bug.
3. **Reduce.** Make each cut in an isolated copy where the check fails. Cut the input, the setup, the callers, the configuration, and the steps of the check, one at a time. Run the check after each cut. Keep only what the failure needs: each part that remains is a possible cause. For a regression, compare a good state and a bad state with the history or a bisection of kinds.md. This step is complete when each part that is left is necessary: its removal makes the check pass.
4. **Test the cause.** Write 3 to 5 ranked hypotheses, because one hypothesis anchors the work on the first idea. Each one states its prediction: "If X is the cause, a change of Y stops the failure." For a direct cause, one hypothesis is enough, but give the reason. Show the list in the chat and in the report. Continue at once. The user can rank it again with domain knowledge.

   Probe one variable at a time, with the probes of techniques.md. This step is complete when evidence links the cause to the symptom and rules out the other hypotheses. If no check that you can run separates them, name the absent evidence and the smallest next check. Then stop with `Blocked`.
5. **Fix.** Remove each probe. Read the skill [tdd](../tdd/SKILL.md), with the Skill tool or as a file. Turn the reduced check into its red regression test. Apply fix-root-causes. Here it sets the place of the fix: the owner of the behavior, and each instance of the same pattern. This step is complete when the regression test failed before the fix and passes after it, or the report says why no test fits.
6. **Verify.** Run the check of step 2 on the full scenario, as often as in step 2. Search for the marker. This step is complete when each item of "Completion conditions" holds.

## Completion conditions

For a fix:

- The reproduction check of step 2 passes on the full scenario.
- The regression test passes, or the report says why no test fits.
- No probe and no throwaway code is in the diff.
- All checks show no failure that the start list lacks.
- The commit message, or the one that the report proposes, states the cause.

For a diagnosis, each fact has its proof level, and the evidence supports the cause and the smallest repair.

## Constraints

- **Contract and documents.** Do not change a frozen acceptance contract, a feature file, a PRD, or a written design. A defect in a contract file is stop reason 1. Give a gap in the scenarios to `to-feature` in the report.
- **Local work.** Apply [guard-one-way-doors](../principles/references/guard-one-way-doors.md). Here it keeps each write local. Do not comment on the bug issue or close it. Only when the user asks, or on an issue branch, commit.
- **Levers.** Apply [build-the-lever](../principles/references/build-the-lever.md). Here it keeps the reproduction script and the bisect wrapper outside version control.

## Output

Write the report in Chinese, with the Chinese terms of `GLOSSARY.md`. Start with `Fixed`, `Diagnosed`, or `Blocked`. Then give:

- the symptom, and the cause with the proof level of each fact;
- for a fix, the changed files and the reason for each;
- each check with its command and result, and each check that you could not run;
- each blocker or risk.

Quote only the log lines that carry the signal. Add the report lines and the report items of the principles that you read, also for `Diagnosed` and `Blocked`.

## Stop rules

- Apply [no-stops-after-approval](../principles/references/no-stops-after-approval.md). Here it limits the questions to its stop reasons.
- If you cannot make a reproduction check, stop with `Blocked`. List what you tried. Ask for access or a redacted artifact (stop reason 3). Or ask for permission to add temporary instrumentation to a real system (stop reason 2).
- Start no later skill, except `tdd` in step 5.
