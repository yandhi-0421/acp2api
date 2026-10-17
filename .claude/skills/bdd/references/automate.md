# Automate the scenarios

This file is the depth of step 3. Read [ste.md](../../domain-modeling/references/ste.md) first. It governs the first line of each commit. If a Gherkin step needs a domain term that `GLOSSARY.md` lacks, load `domain-modeling` at `../../domain-modeling/SKILL.md`. If you lack the Skill tool, read it as a file.

## The runner

Apply [choose-boring-technology](../../principles/references/choose-boring-technology.md). Here it decides the runner: use the Gherkin runner that the project has. If the project has none, use the runner that the written design or the user names. If neither names one, stop with stop reason 4. In the question, give the items that the principle lists, for a runner that you recommend. Suggest that `architect` names the runner.

A test dependency that neither the written design nor the user names is a new technology. Stop with stop reason 4. Install a dependency that one of them names as follows:

- Use the package manager of the project, at project scope, with locked versions.
- Install nothing globally. Add no runner that the project does not track. Add no second runner for the same dialect.
- Confirm each command against the help of the runner or the documents of the project.
- Read the diff of the manifest and the lock file. Remove each unrelated change.

The runner can load a file by a file pattern, a hook, or a data loader. The freeze commit lists each such file, but not the dependency manifest or its lock file, as `freeze.md` says. Commit a change of these two files in the freeze commit.

When the runner allows it, keep the configuration of the runner in a file of its own, as `freeze.md` says. Write the exclusion rule in that file. If the rule is already there, add no second rule. Run the default command. Check that the run excludes the scenarios with `@wip`.

The default command is the command that runs all tests of the project. Make it run the Gherkin runner.

## The seams

Take the seams from the section "Seams" of the written design. If no written design exists, use the narrowest public seam that proves the promise of the scenario. Report that seam as an agent decision.

For domain behavior, prefer an API or a service as the seam. Only when the contract includes an interaction, a visual result, an accessibility result, or another browser result, use a browser.

## The step definitions

Apply [test-behavior-not-implementation](../../principles/references/test-behavior-not-implementation.md). Here it sets the source of the expected value: the scenario, never the product code. Assert through the seam.

- Give one domain meaning one step definition. Only when its meaning does not change, reuse a step definition. Reason: a changed meaning changes the result of every scenario that uses it.
- Parameterize repeated language. Avoid a catch-all pattern: it matches unrelated steps and makes the result ambiguous.
- Keep the business setup visible in the `Given` steps. Use hooks for infrastructure, not for business setup. Reason: a hidden setup hides the rule from the reader.
- Do not call one step definition from another. Call a shared function.
- Keep the step definitions thin. Put the access to the system in an adapter, and the shared comparison in an assertion function.
- For a user interface, use a stable locator: a role, a label, a visible text, or an explicit test ID. Do not bind a test to the page structure or to the style.
- Only when the full representation is the contract, use a broad snapshot.

A frozen scenario stays frozen, also when it belongs to an earlier issue. Add a step definition, a fixture, or a fake that no frozen scenario uses. Run the frozen scenarios again after the addition. A change or a removal of a line in a file that a frozen scenario uses is stop reason 1. An added line that no frozen scenario uses is not a change.

## State and dependencies

- Make deterministic data for each scenario. When scenarios can run in parallel, use unique IDs.
- Reset the state with fixtures, transactions, isolated stores, or cleanup APIs. Do not depend on the run order.
- Use the seams that exist to control time, randomness, generated IDs, and external services. A fake can replace only a dependency outside the scenario contract.
- For asynchronous behavior, wait for an observable state, with a time limit. Apply [fix-root-causes](../../principles/references/fix-root-causes.md). Here it changes the repair of an unstable test: find the cause. A retry or a fixed sleep is not a repair.
- Apply [guard-one-way-doors](../../principles/references/guard-one-way-doors.md) to a scenario that writes to a shared database or a third-party service. Here it decides the run: without a test environment, the run is stop reason 2.
- Apply [protect-secrets-and-personal-data](../../principles/references/protect-secrets-and-personal-data.md). Here it adds a scan before the freeze commit: scan the test files and the test data for a secret.

## The dry run

Run the parser or the dry run of the runner with the command of "The run" in `evidence.md`. The run must list each scenario of the issue that is not blocked, and no other scenario.

Each step of these scenarios must match exactly one step definition. Resolve each undefined and each ambiguous step. The steps of other issues stay undefined. That is not an error.

Note for each step definition if it is new, reused, or changed. Put the result in the report. If a new step definition needs a product interpretation that the scenario and the PRD do not fix, it is stop reason 4.

## The questions

The self-check of test-behavior-not-implementation has four questions. Step 4 asks them. Answer two more questions for each scenario now. Give the evidence of both answers:

1. Does the scenario run alone and in any order?
2. Does a failure point to the behavior, not to the test plumbing?
