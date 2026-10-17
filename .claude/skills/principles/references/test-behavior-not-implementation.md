# Test Behavior, Not Implementation

Through the public interface, assert the result that users observe against a literal expected value. Add or keep a test only if it can fail for a defect.

## Why

Each test costs CI time and review attention. It pays for that cost only if it protects a behavior or a meaningful contract, or catches a credible regression. A test that cannot fail for a defect catches nothing. It also gives false confidence: a regression test that never failed proves the mock, not the fix. A test at a seam that is too shallow to reach the real failure gives the same false confidence.

A test that is coupled to the implementation fails after a change that keeps the behavior, so it blocks valid changes. For example, a test that restates a constant or a prompt fails at each valid edit of that value. The goal is confidence, not the number of tests that you add or delete.

## When it applies

It applies when you write, change, keep, or delete a test, and when you review or audit tests.

Keep these tests, also when they match a sign below:

- a test that independently enforces a public API, protocol, config, migration, storage, security, or release contract;
- a test of call order, when the order is observable behavior;
- a regression test with a credible failure mode;
- a test of a relation across table rows, for example a key that two tables must contain;
- a compile-time type test, for example in a `*.test-d.ts` file;
- a test of an exact value, for example a constant, a prompt string, or source text, if users or external systems depend on that value. Examples are a protocol value, a key, a byte sequence, a path, and a cache key. The test must still pass after a rename of identifiers only. Use source inspection only when it is the cheapest way to protect the value.

In an audit, a sign on an old test makes the test a suspect only. A test that looks like implementation can still be the only test that protects a contract. Do not delete a test only because it is static or slow.

It does not apply to the addition of a new test when no practical test exists for a bug fix or a change. Examples are a test that needs a broad harness, brittle mocks, a slow end-to-end run, production-only state, or vague steps. Do not force such a test, because no new test is better than a bad test.

In that case, run the closest executable check of [prove-it-works](prove-it-works.md). **Evidence:** the seams that you tried and why each fails, and the command and the output of the closest check. The rules to change or delete tests still apply. This exception does not cover a scenario of the [acceptance contract](no-stops-after-approval.md).

## Pattern

### What behavior is

Behavior is an observable result that a named observer can distinguish under stated conditions. The observer is a user, a caller, or an external system.

- An error text, a log line, and a performance number are behavior only when the PRD, a scenario, or an [external user](migrate-callers-then-delete-legacy-apis.md) depends on them.
- A default value is behavior when the observer can see it.
- The public interface is behavior for its external users. Examples are an exported function, a command line, an HTTP endpoint, and a file format. The removal of the old interface of a Parallel Change, after the user checked the date in the removal issue, is no change of behavior.

### Where to test

- Test through a seam that the callers of the code also use. Never test internals.
- Use the narrowest stable seam that still includes the whole behavior, or the mechanism of the failure. Give each contract one primary test at that seam.
- A second test of the contract at a different layer needs a different risk, for example a transport failure.
- To add a case, extend a table of cases or a shared fixture. Do not add a near copy of a test. Merge repeated setup in the same change.

### What to assert

- Call the code under test in the test body with one concrete input. Assert the literal output or the observable effect, for example `expect(slugify("Hello, World!")).toBe("hello-world")`.
- Take the expected value from a source outside the code. Good sources are the requirement, the scenario, a domain rule, a worked example, and a result that you captured before a refactoring. Do not compute the value with production code or with a copy of its algorithm.
- Use one logical assertion in each test.
- For an absence, assert the presence for the other input in the same test.
- For a constant, test the mechanism that reads it, with one input.
- For generated or non-deterministic output, check a schema, an invariant, or a risk-based evaluation. Do not demand the exact sampled text.

| Bad | Good |
|---|---|
| `"checkout calls paymentService.process"`: a mock of the internal `paymentService` asserts that `process` received the total. | `"user can checkout with valid cart"`: make a cart, add a product, check out, and expect the status `"confirmed"`. |
| After `createUser`, a database query looks for the new row. | After `createUser`, `getUser(user.id)` returns the name `"Alice"`. |
| `expect(calculateTotal(items)).toBe(items.reduce(...))` | `expect(calculateTotal([{ price: 10 }, { price: 5 }])).toBe(15)` |

### Mocks

- Mock only at a system boundary: an external API, the database, time, randomness, and sometimes the file system. For the database, prefer a test database. For the file system, prefer the local stand-in of [deep-modules](deep-modules.md), for example an in-memory file system.
- Do not mock your own classes or modules in the same process. Run the real production path.
- For a mock at a system boundary, assert the payload that it received or the state after the call. Do not assert only that the call occurred.

### Regression tests

- Write the regression test before the fix, at a seam that reaches the real bug pattern. If no seam reaches it, do not add a shallow test. Report the absent seam as a finding.
- Do not use a mock that removes the interaction that caused the bug. Keep the boundary value or the event order that exposed the bug.
- Write one regression test, at the seam of the module that owns the behavior. Do not repeat it at each layer that the bug crosses.
- Add the focused test first. Then, if the bug shows a broader class and this needs no large setup, test the class too.
- For an unstable bug, make the test deterministic. Record the signal that the test locks.

### Change or delete a test

- Do not change a test to match a wrong implementation.
- Do not weaken or delete a test to get a green run. This includes its expected values, assertions, fixtures, tags, and step definitions.
- Before you change or weaken an expected value, find the requirement that changed the behavior.
- If a test fails after your change, treat the failure as a possible regression. If a kept test fails on the current code, treat the failure as a possible bug in the production code. In both cases, reproduce the defect. Then fix the module that owns the behavior. Delete or change the test only with the requirement that changed the behavior. For a test that is coupled to the implementation, the evidence of the self-check below is enough.
- Exception: a kept test fails before the first unit, and the issue or the audit does not name it. Record it as [sequence-verifiable-units](sequence-verifiable-units.md) says.
- If no assertion from "What to assert" fits the test, delete the test, unless the keep list names its contract.
- Before you delete a test that is part of the acceptance contract, get the approval of the user.
- After tests at a new interface exist, delete the old tests of the shallow modules behind it. Also delete the tests that protect only implementation details from before a refactoring.

## Self-check

Before you add a test, or when you review a new test, answer the four questions below. If an answer is absent, do not add the test yet.

1. Which observable behavior, invariant, or independent contract does the test protect? **Evidence:** the requirement, the scenario, or the interface, as `file:line`.
2. Which credible regression makes the test fail? This question asks if the test can fail for a defect.
   - First, for a runtime test, ask: if each imported function returned `undefined`, would the test still pass? If yes, change the assertion or delete the test.
   - Then show one run in which the test fails on its assertion, for the intended reason. For a crash bug, the reported exception of the code under test is the intended failure, because it is the reported symptom. For a compile-time type test, the type error on its assertion is the intended failure. Else, a failure of an import, a compile step, or a fixture does not count. If the test passes, or fails for a different reason, correct the test before you change the code.
   - For a bug fix or new behavior, use the run before the change. In other cases, put the regression into the code on purpose, for example invert a condition or revert the fix. Run the test. Then remove the regression as [guard-one-way-doors](guard-one-way-doors.md) says in "Protect the working tree of the user".
   - **Evidence:** the regression, the command, and the failure line.
3. Why does the current coverage not catch that failure already? **Evidence:** the command that searches for other tests of the same contract, its hits, and why each hit misses the failure.
4. Does the test need an export, a flag, a wrapper, or a hook that no production caller needs? If yes, move the test to a seam that production uses. **Evidence:** for each such item, the count of production callers.

Then compare the test with each sign below. A match rejects the test, unless the keep list in "When it applies" names the contract that the test protects. **Evidence:** for each match, that contract.

Before you delete or change a test, find the failure that it can detect, and the stronger proof that stays. **Evidence:** the name of the test, that failure, and the `file:line` of the stronger proof. For a test that cannot fail for a defect, the sign that it matches instead. For a changed expected value, also quote the requirement that changed the behavior. Do the same for a test that failed after your change. For a test of the acceptance contract, also quote the approval of the user.

## Signs that you skipped it

The test cannot fail for a defect:

- It has no assertion, or only a weak one: `toBeDefined`, `toBeTruthy`, `not.toThrow`, or `toBeGreaterThan(0)`.
- It asserts only a mock or an absence: `toHaveBeenCalled`, `toBeUndefined`, `toEqual([])`, or `not.toBe(wrongValue)`.
- It is a tautological test: the code under test, or a copy of its algorithm, gives the expected value.
- It compares a value with itself, or with a copy of itself.
- It restates a constant, a config default, a table row, or a prompt string.
- It restates a copied fixture or list, for example an inventory, a manifest, or an export list.
- It restates a declared flag, but it does not test the delivery that the flag promises.
- It searches the source code for exact text, for example an import or a string.
- The assertion reads data that the test built, and the code under test never runs in the test body.
- A fixture supplies the result that the code under test must produce, or the test asserts a store that the path never writes.
- A mock does the asserted behavior itself, or one identical mock stands in for different APIs.
- A negative control passes for an unrelated reason, for example a denial from a different guard.
- The name or the fixtures promise a behavior that the input does not cause.

The test is coupled to the implementation:

- It mocks internal collaborators, tests private methods, or checks through a side channel, for example a database query.
- It asserts call counts or a call order that users cannot see.
- It breaks after a refactoring that keeps the behavior. In a diff, a refactoring commit also changes test assertions.
- Its name describes how the code works, not what users get.

The test costs more than it protects:

- Two or more tests call the same contract, for example a replay of a shared function in each provider.
- It exists only to keep test-only exports, globals, or wrappers alive.
- It depends on time, run order, or unrelated global state.
- It needs expensive infrastructure for a small fix.
- You would delete it just after it proves the fix.

## Relations

- [prove-it-works](prove-it-works.md): if no practical test exists, it gives the closest executable check.
- [sequence-verifiable-units](sequence-verifiable-units.md): it holds the order of tests and code, and the rule for a commit with a red test.
- [no-stops-after-approval](no-stops-after-approval.md): after an approval, a deletion of a test of the acceptance contract changes that contract. Thus the question to the user is a valid stop.
- [deep-modules](deep-modules.md): a behavior that is hard to test through the interface is a sign of a wrong module shape there.
