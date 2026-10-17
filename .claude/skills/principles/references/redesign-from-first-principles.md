# Redesign From First Principles

At the design step, redesign as if the new requirement was there from the start. At the build step, if a requirement fits only as a bolt-on, stop and return to the design step.

## Why

A structural decision sets the cost of each later change. A bolt-on keeps the old design and puts the new requirement at its side, as special cases. Each later change must then work around those special cases, and each new bolt-on adds more. A redesign gives one structure that fits all the requirements, so later changes stay cheap.

At the build step, a bolt-on can look cheaper than a stop. But it changes the approved design without the approval of the user, and it hides the defect in the design.

## When it applies

It applies when a requirement enters a design that exists, for example a new feature, a new constraint, or a changed rule. It also applies at the build step, when a requirement does not fit the approved design.

The design step is the work that decides the shape of code before you write it. The build step is the work that writes code for an approved design or for a frozen [acceptance contract](no-stops-after-approval.md). If a frozen acceptance contract has no approved design, the approved design in this principle is the design of the code that exists.

A requirement fits only as a bolt-on when the approved design can hold it only with one of these changes:

- a special case in two or more callers;
- a new value that passes through layers that only forward it.

It does not apply when the requirement fits the design that exists without these. Then make the change at the build step, with no redesign.

## Pattern

### At the design step

- Read all affected files. Understand the design before you change it.
- Ask: if we wrote this from scratch with the new requirement, what would we build? Write the answer in the written design or in the report.
- Think through the whole redesign. Then deliver it in increments.
- Plan each increment so that it adds a coherent abstraction, or deepens one that exists.
- Carry the change to each place that names the changed concept. These places include types, docs, examples, tests, and the written reasons for the design.

### At the build step

If the requirement needs one of the bolt-on changes above, look for a direct path in the approved design. If no direct path exists, the requirement fits only as a bolt-on. Stop. Return to the design step with these places as `file:line`.

## Self-check

1. At the design step: what would you build from scratch with the new requirement, and where does the plan differ from it? If it differs, change the plan, or record the reason. **Evidence:** that answer, as a `file:line` in the written design or as a quote in the report.
2. After the change: search for the changed concept in types, docs, examples, and tests. Is each hit updated? **Evidence:** the search command, the count of hits, and the reason for each hit that you did not update.
3. At the build step: does the change add special cases in two or more callers, or a value that a layer only forwards? If yes, and no direct path exists, stop. **Evidence:** each such place as `file:line`, or the search that found none.

## Signs that you skipped it

The change is a bolt-on:

- A new capability spreads across callers as special cases.
- A new value passes through several layers that only forward it to the place that uses it.

The change did not reach each place:

- Types, docs, examples, or tests still describe the design from before the change.

## Relations

- [subtract-before-you-add](subtract-before-you-add.md): it holds the smallest diff at the build step. Its other parts also apply to a redesign.
- [no-stops-after-approval](no-stops-after-approval.md): after an approval, the return to the design step is a valid stop, because the work needs a change to the approved design.
- [sequence-verifiable-units](sequence-verifiable-units.md): it holds the increments of the redesign: small units, and each unit ends green.
- [migrate-callers-then-delete-legacy-apis](migrate-callers-then-delete-legacy-apis.md): if the redesign replaces an interface, it holds the move of all callers and the deletion of the old interface.
