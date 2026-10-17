# The improve mode

Sections: the goal, where to look first, find the cause, the list of opportunities, after the choice, the table of moves.

## The goal

Reduce the cost of the change that the user names. Make the next related change easier and safer. Keep the behavior. If an opportunity or a design changes a behavior, suggest `to-prd` for that behavior. Design only the part that keeps the behavior. Look for deeper modules, for testability and to help an agent to find code.

## Where to look first

If the user names a direction, for example a module, a subsystem, or a pain, take it. If not, read the recent commit history (`git log --oneline`) for hot spots: the files and areas that change often. Look there first, because a deeper module pays only where the code changes again. If the changes are scattered and no hot spot exists, widen the search. If the user names no change, use the recent changes in that area as the change of step 1.

## Find the cause

Ground the direction as [ground.md](ground.md) says. Then find the code that makes the change hard, and the decision that causes that cost.

Do not treat a code smell or a measure as proof. The sign rule of [principles](../../principles/SKILL.md) gives the reason: trace the cost before you report it.

Apply [deep-modules](../../principles/references/deep-modules.md) to each suspect module. Here it changes one thing: the deletion test decides if the suspect is a pass-through or earns its place.

## The list of opportunities

End step 1 with a list. Give for each opportunity:

- the files and modules;
- the pain and its cause;
- the change, in plain words;
- the benefit, in terms of locality and leverage, and how the tests improve.

Use the vocabulary of deep-modules and the terms of `GLOSSARY.md`. Write "the Order intake module", not "the FooBarHandler".

If an opportunity contradicts an accepted ADR, list it only when the friction justifies a new look. Mark it clearly, for example "contradicts ADR-0007, but a new look is justified because ...". Do not list each refactoring that an ADR forbids.

End the list with the top recommendation: the opportunity to explore first, and why. Then stop, as the stop rules of `SKILL.md` say.

## After the choice

Run steps 2 to 5 for the chosen opportunity. Add these parts to the written design:

- the pain and the cause;
- the moves, and the checks that keep the behavior;
- the first vertical slice, which is one complete slice that `to-tickets` can take;
- the risks, as open questions.

Trace the chosen code as [ground.md](ground.md) says, with its history if it may move ownership or layers.

For the recommended design, consider whether to extract or to inline code. Apply [minimize-reader-load](../../principles/references/minimize-reader-load.md) to an extraction and [deep-modules](../../principles/references/deep-modules.md) to an inline. Do not extract or inline only because two blocks look alike. Then select the moves with the table below.

## The table of moves

The table gives options, not answers. Inspect the code before you select a move.

| Signal | Design question | Useful moves |
|---|---|---|
| One rule appears in many callers | Which module should own the rule? | Move Function, Move Field, Combine Functions into Class |
| One change touches many files | Which knowledge must change together? | Move Function, Encapsulate Variable, Change Function Declaration |
| One function mixes different stages | Do the stages use different data or change for different reasons? | Extract Function, Split Phase, Separate Query from Modifier |
| A call chain only forwards work | Does each step hide useful knowledge? | Inline Function, Inline Class, Remove Middle Man |
| Conditions describe stable special cases | Can guard clauses show the main path? | Decompose Conditional, Consolidate Conditional Expression, Replace Nested Conditional with Guard Clauses |
| Cases change independently | Does each case own different behavior? | Replace Type Code with Subclasses, Replace Conditional with Polymorphism |
| Primitive values carry rules | Does the value have an invariant or behavior? | Replace Primitive with Object, Introduce Parameter Object |
| A dependency blocks tests or migration | Must the current task replace or isolate it? | Change Function Declaration, Encapsulate Variable, introduce an adapter |
| Old and new implementations must coexist | Can callers move in safe groups? | Parallel Change, Branch by Abstraction |

The names of the moves follow *Refactoring* by Martin Fowler, second edition. Parallel Change and Branch by Abstraction are migration techniques. An adapter is a design technique. Apply [migrate-callers-then-delete-legacy-apis](../../principles/references/migrate-callers-then-delete-legacy-apis.md) to the last row: it says when the old path and the new path may exist together.
