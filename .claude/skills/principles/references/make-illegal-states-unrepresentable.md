# Make Illegal States Unrepresentable

Before you write logic, choose data structures and types that cannot hold an illegal state of the domain. Let the type checker, not conditions in many places, reject illegal states, mixed-up primitives, and unhandled cases.

## Why

Scattered booleans, repeated shape assumptions, and branches across files are accidental complexity. A structure that fits the domain makes illegal states unrepresentable, and it deletes branches. A case that the types let you ignore becomes a runtime failure that the compiler could stop.

The right structure is cheap when you write the code. Later, the same change looks like a refactoring, and people defer it. Meanwhile, each new feature adds a branch to the wrong structure.

## When it applies

It applies before you write logic, when you choose the core types and data structures. It also applies to stateful logic, to code with many branches, and to a shape assumption that repeats across files. In a typed language, it also applies when you design a type or review a signature. The rules for data structures apply in each language. The rules for types apply in each typed language, and the examples use TypeScript.

Add structure only for a problem that exists now:

- Distrust an abstraction that adds indirection, but removes no branch, rule copy, illegal state, or lifecycle risk.
- For Data Clumps, add a type only for values that form one domain concept now. Do not add a parameter object only to cut the count of parameters.
- For Primitive Obsession, add a domain type only to enforce an invariant that exists now.
- For Repeated Switches, if the cases own behavior and change independently, use polymorphism. For one stable rule, use a simple conditional.
- Make a type only as strong as its use sites need. The type tracks the cases that each use site must handle, not each detail of the data.

It does not apply when the shape is clear, confined to one module, and unlikely to grow. Then plain code is better than a forced abstraction. Confined to one module means that only one module reads and writes the data. If a second module reads the data, or the requirement adds a state, the principle applies.

## Pattern

### Data structures first

- Define the core types early. Trace each access pattern. Choose the structure that fits the main paths. For example, an access pattern can need a queue, a cache, an index, a graph, a tree, or a normalized collection.
- Put the domain into a structure, not into conditions in many places:

| Instead of | Use |
|---|---|
| scattered booleans, phases, or lifecycle guards | a state machine |
| loose parameters or repeated shape assumptions | a typed model |
| branches on one value across files | a registry, a lookup table, a sum type, or the dispatch pattern of the codebase |
| state changes from many places | a reducer, or a model of commands and events |
| repeated behavior, ownership, or invariants | a small module that gathers them |

- If no structure fits, write down what the code must never allow and how it reads the data. Then find the structure that encodes exactly that.
- Keep one list of the same items. Derive the other lists from it. If you cannot derive a list, make the build fail when the lists differ.

### Types

- Model exclusive states as a sum type, not as optional fields that allow contradictions.
- Build a type from the values that you want. Do not cut them out of a looser type with guards.
- When two values share a primitive type but mean different things, brand each one, for example `UserId` and `OrderId`. Validate the value once, in the function that creates it. Then trust the type.
- Make each match exhaustive, so that the compiler fails when a new case has no handler.
- Derive each type from its authoritative schema: protobuf, OpenAPI, GraphQL, a database migration, or design tokens. Do not write a parallel type.
- Define errors out of existence. Remove special cases in the same way. Do not add more handlers.
- Treat a runtime assertion, a null check, or a "never happens" throw as the mark of a weak type. Move that guard into the type.
- Do not lie to the compiler. A cast or an unsafe coercion is a latent crash. Prove the fact with a guard or a narrower type. If you cannot, name the hazard in a comment at the cast.
- Prefer total functions.

| Bad | Good |
|---|---|
| `{ completed: boolean; completedAt?: Date }` allows `completed: true` with no date. | `{ kind: "open" } \| { kind: "done"; at: Date }`. Or keep only `completedAt`, and derive `completed` from it. |
| A list, and a guard on its length before each read of the first item. | A non-empty list: `{ head: T; rest: T[] }`. |
| A start time and an end time that each caller must keep in order. | A start time plus a duration. |
| `assign(userId: string, orderId: string)` | `assign(userId: UserId, orderId: OrderId)` |
| `head(items: T[])` throws on an empty list. | `head` takes a non-empty list. `sum` keeps `T[]`, because the sum of no items is 0. |

## Self-check

For the data that the change adds or changes, answer these questions.

1. Does one fact live in two places? Examples are a second boolean that must match the first, and a second list of the same items. **Evidence:** the search command for the fact, its hits, and for each copy the derivation or the build check that keeps it equal.
2. For each field that decides a state, how many files branch on it? **Evidence:** the search command for conditions on that field, and the count of files. If more than one file has the same branch, replace the branches with one structure from the table above. Do this only for the branches that the diff adds. For the branches that existed before, record a [draft follow-up issue](no-stops-after-approval.md) in the report.

In a typed language, also answer these questions for each type that the change adds or changes.

3. Can you write a comment on when a combination of its fields is valid? If yes, split the type into a sum type. **Evidence:** the type as `file:line`, and each illegal combination that no longer compiles.
4. Do two parameters share a primitive type but mean different things? If yes, brand them. **Evidence:** each such signature as `file:line`, with its branded types.
5. Where did each `any`, `as`, non-null assertion, or `assertNotNull` in the diff come from? Trace each one to the place where its value comes from. If that place is a system boundary, parse the data there. If not, make the type stronger. **Evidence:** the search command over the diff, and the count of hits. For each hit, the `file:line` of the parse or of the stronger type, or the comment that names the hazard.
6. If a new case arrives, will the compiler show where to add it? Add a case to the sum type on purpose. Run the type checker. Then remove the case as [guard-one-way-doors](guard-one-way-doors.md) says in "Protect the working tree of the user". **Evidence:** the command, and one error line for each match on the type. A match with no error hides the new case in a default branch.
7. Does the type copy a shape that another file owns? If yes, derive it. **Evidence:** the schema, and the `file:line` that derives the type from it.
8. For each stronger type that the change adds, for example a non-empty list: is it for totality, or only for precision? This question does not apply to the branded types of question 4. **Evidence:** the operation that would fail without it, as `file:line`. If no operation would fail, keep the plain type.

## Signs that you skipped it

The domain lives in conditions:

- A feature adds a branch to an if/else chain, or a second boolean must match the first.
- The diff shows Repeated Switches, Data Clumps, or Primitive Obsession.
- Hand-synced list: two or more places list the same items, and a change edits only one list.

The type is too loose:

- A bug makes you ask: "Can this combination really occur?"
- The diff adds `any`, `as`, a non-null assertion, or a "never happens" throw.
- A match on a sum type has a default branch, so a new case compiles without a handler.
- A type repeats by hand a shape that a schema owns.

## Relations

- [boundary-discipline](boundary-discipline.md): it holds where external data becomes a domain type. A branded type or a sum type gets its first value from the parse there.
- [deep-modules](deep-modules.md): it holds the fixes that change a module interface or a seam, and the sign of temporal decomposition. A fix of the data shape stays here.
