# Minimize Reader Load

Decrease reader load on two axes: the layers to trace, and the hidden or mutable state to hold. Add a layer or a piece of state only when it removes at least as much reader load elsewhere.

## Why

Code is read more often than it is written, so most of its cost is the work of its readers. Line counts, cyclomatic complexity, and "clean architecture" are only proxies for that work. A reader can hold only a few facts at one time, and the next reader can be a person or an agent. The engineer who maintains the code next is a user of it too. If that engineer would find the code hard work to maintain, the solution is bad.

The two axes are independent. A flat file with 50 globals can be as hard to follow as six layers of adapters. A fix on one axis can make the other axis worse, so check both.

## When it applies

It applies when you write, shape, or review code that a reader must trace. It also applies when you want to add a layer, a wrapper, an abstraction, or a piece of state.

It does not apply to lines that only look similar. Remove repeated structure, not each similar line:

- Three similar statements are better than an early abstraction.
- When an extraction adds more concepts than it removes, keep the local repetition.
- Extract a shared rule only when its copies can drift apart, and the rule must change in one place.

## Pattern

### Layers

- Collapse each layer that costs more than it saves, for example speculative indirection. For pass-through methods, adapters, and ports, [deep-modules](deep-modules.md) decides if a layer earns its place. A port at a system boundary is not a layer to collapse.
- Make each decision in one place, behind one owner. Give the result to the other places, for example as a flag. Small repeats and leaks grow into permanent costs of coordination. Do this now in [the code that the change touches](subtract-before-you-add.md). In a fix issue, [sequence-verifiable-units](sequence-verifiable-units.md) moves this refactoring to a draft follow-up issue.
- If one change needs edits in many files, move the shared policy to the part that owns it. Keep rules and data that change together in one place.
- When the new name shows the intent, or when the new module owns one body of knowledge, extract the code.
- Prefer explicit code to clever code.

### State

- Shrink the scope of state. Prefer pure functions to mutations, locals to fields, fields to module state, and module state to globals.
- Return a result. Do not change state as a side effect. For example, write `calculateDiscount(cart)` that returns a discount, not `applyDiscount(cart)` that changes the cart.
- Derive a value from its source. Do not keep two copies in sync.
- State an invariant once, at the interface of the module that owns it, not in each consumer. Then the reader learns it once.

### Names

- Give each item a name that shows what it does or holds. If no clear name fits, the item can mix two roles. Then split the roles.

## Self-check

Apply the 30-second test to each value X that the change adds or changes. The test asks: can a new reader say where X comes from, and what can change X, in under 30 seconds? If not, cut layers or state.

1. Where does X come from? Trace X from its use to its origin. Stop at an interface that answers the question. If the trace passes more than 3 files or layers, flatten the layers that the diff adds. For the layers that existed before, record a [draft follow-up issue](no-stops-after-approval.md) in the report. **Evidence:** the trace as a list of `file:line`, and its count of files or layers.
2. What can change X? Find each place that writes X. If the scope of X is wider than its writers need, shrink it. **Evidence:** the search command, and each writer as `file:line`.
3. Can direct code solve the problem with less indirection? For each layer or piece of state that the diff adds, name the reader load that it removes elsewhere. If it removes none, remove it. **Evidence:** the `file:line` of the code that it makes simpler.

## Signs that you skipped it

The reader must trace too far:

- An answer needs a trace through more than 3 files or layers.
- Several places compute the same decision from the same inputs.
- Shotgun Surgery.
- Duplicated Code.

The reader must hold too much state:

- A global or a module-level variable that only one function uses.
- A function that returns nothing, but changes its argument or a field.
- Each consumer has its own guard for the same invariant.

The code hides what it does:

- Mysterious Name.
- A dense expression or a trick that needs a comment to say what it does.

## Relations

- [deep-modules](deep-modules.md): it decides which layer earns its place, with the deletion test and the count of adapters at a seam.
- [separate-before-serializing-shared-state](separate-before-serializing-shared-state.md): if two modules write X, it gives X one owner.
- [make-illegal-states-unrepresentable](make-illegal-states-unrepresentable.md): it holds the check for one fact in two places, and the data shapes that remove the copy.
