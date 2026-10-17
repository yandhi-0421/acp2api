# Red and green evidence

This file is the depth of step 4. It defines the three classes of a scenario: trusted red, existing green, and blocked. It does not replace `test-audit`, which later audits the link from FR to test and runs mutation testing.

## The run

Select the scenarios with the slice tag of the issue. The command must also run the scenarios with `@wip`, which the default run excludes. The form depends on the runner, so confirm it against the help of the runner.

Run each scenario, and each row of an `Examples` table, from a known state. Treat each row as a scenario of its own. Use the smallest command that isolates its cause.

## Trusted red

A scenario is a trusted red when all of these hold:

- The setup and the access to the system succeed.
- The action reaches the seam.
- The failure is at the assertion that compares the expected value with the actual value. This is the run that question 2 of test-behavior-not-implementation requires.
- The actual value shows absent or wrong behavior.
- A second run fails for the same cause.

These failures are not a trusted red, because none is a failure of the assertion:

- an error at an import;
- an error at a call to a seam that does not exist;
- a failure in a step definition, a fixture, or the environment.

After an error at a seam that the product lacks, go back to unit 2 of step 2 in `start.md`. Add the seam with no behavior. Run the scenario again.

After any other error, repair the test file. Run the scenario again.

A timeout counts only when the assertion proves a missed deadline that the PRD or the scenario states. A timeout of the runner or of the infrastructure does not count.

The runner can skip the steps after the failed assertion. That does not break a trusted red. Reject a `pending` step, an unresolved step, and a skip of the assertion itself.

## Existing green

A scenario that passes at the first run is an existing green only when all of these hold:

- The assertion reads the real response of the system.
- The expected value comes from outside the code.
- A planted regression makes the scenario fail on its assertion, for the intended reason.

Put the test files and the planted regression into an isolated copy of the tree. The section "The list of failures on a rerun" of `resume.md` names the ignored files that a copy lacks. Make the ignored files of the copy, as the last paragraph of that section says. Apply [guard-one-way-doors](../../principles/references/guard-one-way-doors.md). Here it decides where the check runs: in an isolated copy, and not in the working tree. Show the failure line.

If one of the three proofs fails, repair the test file. Run the scenario again. A pass that the empty seam causes is an existing green only when the three proofs hold.

Change no product file in the working tree to make a red or to plant a regression. The empty seam of `start.md` is the one exception. Reason: a red that you made tells you nothing about the product. A script that plants the regression is a lever. Apply [build-the-lever](../../principles/references/build-the-lever.md). Here it decides where the script stays: unless the written design or the issue names it, keep the script outside version control.

An existing green needs no change of product behavior.

## Blocked

A scenario is blocked in one of these cases:

- You cannot observe its result.
- Its test cannot reach a trustworthy assertion without a product change other than an empty seam.
- It has invalid Gherkin, results that conflict, or an open product decision.

"No practical test exists" does not excuse a scenario of the acceptance contract. Keep `@wip` on a blocked scenario. Report its reason and its owner, as `stops.md` says. Do not claim that the issue is done.

If each scenario is blocked, make no freeze commit. Skip steps 5 to 7. Write the report.

## The review of each test

Apply [test-behavior-not-implementation](../../principles/references/test-behavior-not-implementation.md). Here it reviews each scenario that you automated: answer the four questions of its self-check. The failure line of a trusted red is the evidence for question 2. If an answer exposes a weak test, repair the test file. Run the scenario again.
