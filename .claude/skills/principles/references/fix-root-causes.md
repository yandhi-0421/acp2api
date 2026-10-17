# Fix Root Causes

Fix each defect at its root cause, in the layer that owns the behavior. Do not hide a symptom or swallow an error.

## Why

Symptom fixes accumulate. A guard, a retry, or a catch at the symptom hides the first wrong state, and the real defect stays. Each such fix makes the system harder to understand. A root-cause fix costs more at first, but it costs less in total.

Without a method, an agent changes the first likely cause and uses a green suite as proof. A reproduction check that fails on this bug leads to the cause. Without such a check, a study of the code does not find the cause.

When two or more fixes that share one premise fail, each failure is evidence about that premise.

## When it applies

It applies when you debug: a crash, a wrong output, an unstable test, a performance regression, or an unexpected failure of a check. It also applies when you add or review a catch block, a retry, or a default value on error inside the system. It also applies when two or more fixes that share one premise fail the same check.

A mitigation, for example a rollback, is not a root-cause fix. After a mitigation, this principle still applies.

It does not apply when a new requirement enters the design, because a new requirement is not a defect.

It also does not apply to a check that failed before the first unit, if the issue does not name it. A failure at the start that is an instance of the defect that the issue fixes counts as named by the issue. Record an unnamed failure as [sequence-verifiable-units](sequence-verifiable-units.md) says.

## Pattern

### Reproduce first

- Before you form a hypothesis, make a reproduction check: one command that fails on the reported symptom.
- Make the check assert the exact symptom that the user reported, not a nearby failure. A wrong bug gives a wrong fix.
- Make the check deterministic, fast, and able to run without a person.
- If the bug does not occur on each run, make the check reproduce it at a high rate. Give the rate.
- Run the check before the fix. See it fail for the reported reason.
- Do not change production behavior because of an explanation that you did not test.

### Trace the cause

- Trace data and control flow back from the symptom to the first wrong state.
- Ask why until you reach the root cause.
- If you cannot find the cause, add logs. Read the actual error. Do not guess.
- If something fails after a restart, suspect stale persistent state first: config files, caches, lock files, and saved state.
- If the correct behavior returns after you reset a state file, make the validation of that state the fix.

### Fix at the owner

- Make the narrowest coherent change in the layer that owns the behavior.
- Search for the same pattern. Fix each instance, not only the instance that you found.
- Each instance of the same defect is part of the fix, not an unrelated problem. If an instance needs work beyond the approval, that is stop reason 4 of [no-stops-after-approval](no-stops-after-approval.md).
- Do not add a guard that silences a crash. A null check at the symptom is a symptom fix.
- Do not add a retry, a guard, an abstraction, or a compatibility path without evidence that the bug needs it.
- Inside the system, do not catch an error and continue. Do not return a default value instead of an error.
- Do not use a retry to hide an unrelated or unstable failure.
- For an order defect, wait for an event, a state, or an explicit synchronization rule. A fixed sleep is not a fix.

### After two failed fixes with one premise

- If two or more fixes that share one premise fail the same check, suspect the premise, not the fixes.
- Write the premise down: the one sentence that each failed fix assumed.
- Test the premise before the next fix.
- For an imbalance across actors, for example workers, nodes, or shards, the test is a census. Count the imbalance for each actor. The census shows which actors hold the imbalance, not its size.
- Write the census as a script that can run again.
- If the same few actors hold most of the imbalance in each run, find what gives them that role. That is the next "why".
- Remove the asymmetry. Do not compensate for it. Rotate the role, randomize the assignment, or move the role, so that no actor holds it in each run.
- If the census is even across actors, the premise is not the cause. Look for the cause elsewhere. Keep the census as evidence.

## Self-check

Before you say that a fix is complete, answer questions 1 to 5. Answer question 5 also for each diff that adds a catch block, a retry, or a default value on error. After two failed fixes with one premise, answer question 6 before the next fix.

1. Which command reproduces the reported symptom? **Evidence:** the command, and the failure line of its run before the fix. The line shows the symptom that the user reported.
2. What links the cause to the symptom, and what rules out the other causes? **Evidence:** the probe or the controlled change, and its result. For each other cause, the observation that rules it out. If the evidence cannot separate the causes, the list of causes that stay open.
3. Where else does the same pattern occur? **Evidence:** the search command, its count of hits, and for each hit: fixed, or the reason why it is different. For each fixed hit, also a check that fails before its fix and passes after it.
4. Is the reported symptom gone? Run the reproduction check again, on the full scenario, not on a minimized one. **Evidence:** the command and the output line that shows the pass.
5. Does the diff swallow an error? Search the diff for new catch blocks, retries, and default values on error. Each one must propagate the error to the caller. If not, it must be at a named system boundary, and return or raise the error there. **Evidence:** the search command and its count of hits. For each hit, its `file:line`, the name of its system boundary if it has one, and the line that propagates, returns, or raises the error.
6. Did you test the premise before the next fix? **Evidence:** the premise in one sentence, and the command and the output of its test, for example the census.

## Signs that you skipped it

The fix hides a symptom:

- A guard, a retry, or a catch at the symptom.
- A default value that replaces an error inside the system.
- A fixed sleep as the fix of an order defect.
- A workaround needs a long comment to justify it.
- For an imbalance across actors: a return path, a shared pool, a batched hand-off, or a periodic rebalance. It keeps the role of an actor, and it adds work in each run.

The cause has no proof:

- You read code to build a theory before the reproduction check exists.
- A broad green suite replaces the reproduction check of the reported symptom.
- A third fix assumes the same premise as two fixes that failed.

## Relations

- [boundary-discipline](boundary-discipline.md): it defines the system boundary, and it holds the rules for errors there. Validation and error handlers at a system boundary are necessary, and they are not symptom fixes.
- [test-behavior-not-implementation](test-behavior-not-implementation.md): it holds the regression test of a fix, and its seam. The reproduction check here is often the start of that test.
- [build-the-lever](build-the-lever.md): the census script is a lever. That principle holds where the script stays.
- [subtract-before-you-add](subtract-before-you-add.md): a removal of the asymmetry is a subtraction. Prefer it to a new mechanism that compensates.
- [redesign-from-first-principles](redesign-from-first-principles.md): it rebuilds a design for a new requirement. The premise test here questions a fact that the current design assumes.
- [protect-secrets-and-personal-data](protect-secrets-and-personal-data.md): it holds the redaction rules. Apply them to each log that you add and to each output that you show.
- [guard-one-way-doors](guard-one-way-doors.md): a fix that changes stored data, a published contract, or a security rule is a one-way decision. That principle holds the options that go to the user before such a fix.
