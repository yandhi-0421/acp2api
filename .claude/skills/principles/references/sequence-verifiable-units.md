# Sequence Verifiable Units

Do the work in small units, each with one purpose and a check that passes before the next unit starts. End each issue, or each direct request, with all checks green.

## Why

A break that you find in the unit that caused it is cheap to locate. A break that you find after a batch is under later work, and that work stands on a broken base. A sequence of checked units also lets a reviewer replay the work: "trust me" becomes "watch it go red, then green".

A unit with two purposes hides one change behind the other. The diff grows, the check loses its focus, and a revert removes both changes.

## When it applies

It applies to work with more than one step, for example a migration, a large refactoring, or a series of similar edits. It also applies to the order of commits and pull requests, and to each diff that you make or review.

A unit is one change and its check, for example one edit in a series of similar edits, or one red-green cycle. If edits work only together, for example the edits of a rename, they are one unit.

Do not plan a break that a later issue repairs. This rule also applies to a migration in phases: a good end state does not excuse a red state on the way.

It does not apply in these cases:

- A test fails as the expected red of a TDD or BDD cycle. That test is not a break, if its fix comes in the same issue.
- A change has one step and no code behavior, for example a typo in a document. For such a change, run the smallest relevant check of [prove-it-works](prove-it-works.md).

## Pattern

### Units

- Make each unit a bracket: a known-good state, one change, the check, and then the next unit.
- If the branch exists only on this machine and has no uncommitted changes, rebase it onto the current default branch. If not, do not rebase. If the skill that applies this principle forbids a rebase, do not rebase. Give the base in the report.
- Before the first unit, run all checks. Record each failure by name, for example a test ID or a lint hit. Then each check measures against the real start state.
- Fix each failure at the start that the issue names. A failure that is an instance of the defect that the issue fixes counts as named by the issue.
- A failure at the start that the issue does not name is an unrelated problem. Record it as a [draft follow-up issue](no-stops-after-approval.md) in the report. Do not fix it.
- Exception: a failure at the start can prevent a check that the issue needs. Then it is not unrelated. Before an approval, put its fix into the plan. After an approval, it is stop reason 4 of [no-stops-after-approval](no-stops-after-approval.md).
- End the issue with all checks green, except the failures from the start. At the end, compare the names of the failures with the list from the start. No test or check that passed at the start may fail.
- A test that passed at the start and is absent at the end counts as a failure. The exception is a deletion with the evidence that [test-behavior-not-implementation](test-behavior-not-implementation.md) requires.
- Do first the work that helps each later phase, for example CI, lint, test infrastructure, and shared types.
- Work in vertical slices: one test, the code that makes it pass, and then the next test. Let each cycle use what the last cycle showed.
- If a lever makes the edits, the check of each unit costs almost nothing. Run it anyway.
- Before a structural change, make a repeatable check of the behavior that the change must keep. If no test covers that behavior, add one first. If no practical test exists, use the closest executable check of [prove-it-works](prove-it-works.md).
- Make each refactoring step keep the behavior. Give each behavior change its own requirement and check. Make that check pass before the next structural change.

### Commits

- Order the commits and the pull requests so that they prove the work. The usual order is a test that fails, and then the fix.
- Other orders also prove the work: a removal before the change of structure, a measurement before the treatment, and the scaffold before the feature.
- Keep each commit small, with one purpose. Then the sequence reads as an argument.
- Make each commit pass the check of its unit. The rule has one exception: inside one issue, a commit can add a red test before the commit of its fix.
- At the end of each issue, run all checks: the full static and runtime checks of the project.

### Scope

- Map each hunk of the diff to the issue.
- Do not mix a fix with a refactoring, format changes, cleanup, or unrelated defensive changes in one unit.
- A removal in [the code that the change touches](subtract-before-you-add.md) is its own unit. Do it before the change.
- In a fix issue, finish the fix first. Record each refactoring other than a removal as a draft follow-up issue in the report. This rule also covers the code that the change touches.
- If you see an unrelated problem, do not fix it in this issue. Record it as a draft follow-up issue in the report.
- An instance of the defect that the issue fixes is not an unrelated problem. [fix-root-causes](fix-root-causes.md) holds this rule.
- Keep a regression test on its bug. Do not add broad fixture changes or unrelated coverage to it.

## Self-check

At the end of each issue, answer these questions.

1. Which units did the issue have, and which check closed each unit? **Evidence:** a list of the units, each with its check command and its result.
2. For each commit with a red test: did the test fail for the intended reason, and did the fix make it pass? **Evidence:** the command and the failure line before the fix, and the run that passes after it.
3. Does the last commit of the issue pass all checks, except the failures from the start? **Evidence:** the command of all checks, the lists of the failures at the start and at the end, and their difference. Also each test that the diff deletes, with the evidence for its deletion, and the commit hash of the end.
4. Does each hunk serve the issue? Map each requirement to the code that implements it. Map each delivered behavior to its source in the issue or the design. **Evidence:** each requirement with its `file:line`, and each changed behavior with its requirement. Also the command that shows the final diff, and each hunk that you removed or recorded as a draft follow-up issue. A removal unit in the code that the change touches also serves the issue.

## Signs that you skipped it

The work has no check after each unit:

- Many edits, and then one check at the end. A failure appears far from its cause.
- A horizontal slice: all tests first, and then all code. Such tests check the shapes that you imagined, not the behavior. Exception: when you automate the scenarios of the [acceptance contract](no-stops-after-approval.md) of one issue, you can automate them together. In the inner loop of TDD, the tests still come one at a time.
- A commit fails its check, and it is not a red test before its fix.
- A test or a check that passed at the start fails at the end of the issue, or the test is gone.
- A plan accepts a break that a later issue repairs.
- A report claims a red test, but it shows no failure output.

The diff has more than one purpose:

- Scope creep: behavior in the diff that nobody asked for. When it breaks the agreed scope, or adds a concrete risk or cost, report it.
- A fix commit also refactors, reformats, or cleans up unrelated code.
- A regression test comes with broad fixture changes or unrelated coverage.

## Relations

- [prove-it-works](prove-it-works.md): it keeps each check real. The check that closes a unit observes the real result, not a proxy.
- [build-the-lever](build-the-lever.md): it makes each check cheap, because a lever does the edits and runs again. A lever does not remove the check of each unit.
- [test-behavior-not-implementation](test-behavior-not-implementation.md): it decides what counts as a red run. A red-test commit needs a failure on the assertion, for the intended reason.
- [migrate-callers-then-delete-legacy-apis](migrate-callers-then-delete-legacy-apis.md): it decides how a migration fits in one issue. Thus a migration needs no planned break.
- [no-stops-after-approval](no-stops-after-approval.md): it holds the stop for work beyond the approved scope, and the record of an unrelated problem as a draft follow-up issue. A hunk that needs work beyond the approval is a reason to stop, not a silent addition.
- [guard-one-way-doors](guard-one-way-doors.md): a push to a remote is a one-way door. That principle holds its confirmation, and the steps that one confirmation covers.
