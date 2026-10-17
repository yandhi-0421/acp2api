# Slice the work

Sections: the slice, the first slice, scenarios, scenarios not sliced, preparatory refactorings, the refactoring issue, a migration, the improve mode, blocking edges.

## The slice

A slice is a vertical slice: a narrow but complete path through each layer that the behavior needs. A horizontal slice builds one layer. Its scenarios cannot pass alone, so it cannot be complete.

A slice is complete when its scenarios pass. It then shows its behavior on its own.

Each slice fits in one fresh context. A fresh subagent sees only the issue, its scenarios, and the written design. If one subagent cannot hold these and the code that the change touches, split the slice.

## The first slice

Make the first slice the thinnest path through all layers. It needs the fewest scenarios that run one real request from input to result. It proves that the layers connect. Later slices add scenarios to a path that works.

## Scenarios

Choose a slice by the behavior from end to end, not by the FR. One FR can fill several slices, and one slice can hold scenarios of several FRs. Each scenario is in exactly one slice. A `Scenario Outline` is one scenario, so all its rows go into one slice.

A scenario with a slice tag is sliced. A sliced scenario stays in its issue. In a rerun, slice only the scenarios that have no slice tag.

## Scenarios not sliced

Slice only the scenarios that no blocker affects. List each other scenario in the report with its reason. These scenarios are not sliced:

- a scenario that a blocker of a `Blocked draft` affects;
- a scenario of an FR that the written design lists as not designed;
- a scenario of a retired FR (report it with the owner `to-feature`).

Read the blockers in "Open decisions" and the measurement gaps in "Success criteria" of the PRD. The FRs that are not designed are in the section "Problem" of the written design.

## Preparatory refactorings

The written design lists the preparatory refactorings in its section "Preparatory refactorings and migration shape". Make each one the first unit of the first slice that needs it. If it does not fit into that slice, make a refactoring issue for it. Fit means that the slice still fits in one fresh context. Add a blocking edge from that issue to the slice.

## The refactoring issue

A refactoring issue has no scenarios and no slice tag. It keeps the behavior. [issue-template.md](../assets/issue-template.md) gives its "Done when" lines.

A removal issue is a refactoring issue that removes the old interface of a Parallel Change. Its "Done when" has one more line.

Make a refactoring issue only in these cases:

- The written design is of the improve mode, so it has no scenarios.
- A preparatory refactoring does not fit into the first slice that needs it.
- A Parallel Change needs a removal issue.

## A migration

Apply [migrate-callers-then-delete-legacy-apis](../../principles/references/migrate-callers-then-delete-legacy-apis.md). Here it changes the plan: one issue moves all internal callers and deletes the old interface. The issue that moves all internal callers is the slice that first needs the new interface. A preparatory refactoring can carry the move: as the first unit of that slice, or, if the unit does not fit, as a refactoring issue.

If the written design uses a Parallel Change that the principle allows, make a removal issue. Add a blocking edge to it from each issue that moves the callers. If the removal waits for a date, write in the body: "The user checks the date before the removal." The label rule of step 4 then gives `ready-for-human`.

## The improve mode

A written design of the improve mode has no PRD, no FRs, and no scenarios. Thus each issue is a refactoring issue. Group the moves of its section "Moves and checks" into these issues. Apply sequence-verifiable-units as step 2 of `SKILL.md` says. Make the first vertical slice of the design the first issue. The checks of the design that keep the behavior are the "Done when" lines.

If the written design changes a behavior, stop. That behavior needs an FR and scenarios first. Suggest `to-prd`. A removal issue is no change of behavior, as [test-behavior-not-implementation](../../principles/references/test-behavior-not-implementation.md) says.

## Blocking edges

If B cannot pass its checks without the result of A, add a blocking edge from A to B. Otherwise, add none. Write one line for the reason of each blocking edge. These are the usual reasons:

- B calls an interface, or reads data, that A creates;
- B needs the structure that a preparatory refactoring in A makes;
- the removal issue waits for the issues that move the last callers;
- the migration shape of the written design orders the two issues.

An edge for the order in which you wrote the issues makes one long chain. If an edge is absent, an issue can start before its base exists.

Check for a cycle. Use the draft IDs as the node names, without spaces. Write one line "A B" for each edge, where A blocks B. Write one line "X X" for each issue without an edge.

In a rerun, also write the lines for the published issues. Name each one `#<number>`. Read the numbers as "A rerun" in publish.md says.

Give the lines to `tsort`. A cycle makes `tsort` print an error that names the issues of the cycle. Do not read the exit status, because it differs between systems. If `tsort` reports a cycle, do not publish in its order. Merge the issues of the cycle into one slice, or remove a blocking edge that has no reason.

Without a cycle, `tsort` prints the issues in dependency order. Publish in that order. In a rerun, create only the new issues.

The first frontier is every open or draft issue that no other open or draft issue blocks. This skill only names it. The skill `implement` works the frontier.
