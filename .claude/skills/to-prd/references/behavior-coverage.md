# Cover the branches of a behavior

Only when a behavior in scope changes by permission, threshold, lifecycle state, or external failure, use this guide. The guide exposes hidden product choices. It does not turn the PRD into test cases.

## Select the branches

Start with the normal outcome. Only when a condition changes an observable result or a product state, add a branch. Ignore a condition that gives the same result. If the source does not define a branch, record an open decision.

For each branch, state the condition, the observable result, and the state after the branch. Only when a failure needs a next action, add a recovery. Put the branches into FRs by the FR rules in `SKILL.md`.

## Common branch types

- **Threshold:** Distinguish only the sides that give different results.
- **Lifecycle:** State the allowed transitions and the blocked results.
- **Permission:** State the result for an allowed actor and for a denied actor.
- **External failure:** State what the actor sees, which state remains, and whether a recovery changes it.

## Completion condition

Coverage is complete when all of these are true:

- two readers reach the same pass or fail result for every condition in scope;
- no conditions conflict;
- each unknown is an open decision.

If only a trial can show how a branch must behave, recommend the skill `prototype` in its open decision. The PRD records the verdict of the user, not the prototype.
