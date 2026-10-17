# Deep Modules

Put much behavior behind a small interface, at a seam that callers and tests both use. Treat as interface all that a caller must know, for example the invariants, the order constraints, and the error modes.

## Why

A deep module gives leverage to its callers: each fact that a caller learns about the interface gives access to much behavior. It gives locality to its maintainers: a change, a bug fix, or a check occurs in one place, not in each caller. A small interface also needs fewer tests and a simpler test setup. A shallow module gives none of this, because its callers must learn the interface and the implementation.

Code that a caller can reach becomes part of the interface. An agent takes the shortest path that compiles, so it imports each internal part that it can reach. A seam also adds a layer for each reader, so it pays only where behavior varies across it.

## When it applies

It applies when you add a module, change an interface, or place a seam. It also applies when you review such a change.

Measure depth as leverage, not as a ratio of implementation lines to interface lines. A deep module is not a deep call chain. A chain scatters knowledge across layers, but a deep module concentrates capability behind one interface.

It does not apply to code that shares no knowledge. Do not merge such code into one module only to make the module deep.

## Pattern

### Words

Use these words exactly in designs, reviews, and reports. Do not use component, service, or API in their place. Do not say boundary for a seam, because a system boundary is a different concept.

- **Module:** anything with one interface and an implementation, at any scale: a function, a class, a package, or a part that crosses tiers.
- **Interface:** all that a caller must know to use the module correctly. This includes the types, the invariants, the order constraints, the error modes, the configuration, and the performance. It is more than a keyword or a list of public methods.
- **Depth:** the behavior that a caller can use for each fact about the interface that it learns. Depth is a property of the interface: internal parts and internal seams are not part of it. A module is shallow when its interface is nearly as complex as its implementation.
- **Seam:** a place where behavior can change without an edit at that place. The interface of a module is at a seam. Where the seam goes is a separate decision from what goes behind it.
- **Adapter:** a concrete thing that satisfies an interface at a seam. The word names a role, not the content.
- **Port:** the interface at a seam that adapters satisfy. The word comes from ports and adapters.
- **Leverage** is what callers get from depth, and **locality** is what maintainers get. The section "Why" gives both.

### Shape the interface

- Organize a module around one body of knowledge, not around the order of execution. Methods that run at different times belong in one module when they protect the same decisions.
- Hide meaningful decisions behind the interface.
- Make each layer change the abstraction. Inline a layer that only forwards calls or renames an operation, or move the work to its owner.
- Keep a forward only when it adds policy, adaptation, compatibility for an external user, or a distinct abstraction. When its removal makes the ownership unclear, also keep it. [migrate-callers-then-delete-legacy-apis](migrate-callers-then-delete-legacy-apis.md) defines an external user, and it deletes each forward that only internal callers need.
- Make internals unreachable from outside the module, so that an import from outside fails the build.
- Do not expose an internal seam through the interface only so that tests can use it.

### Move knowledge to its owner

- If a function uses the data of another module more than its own data, move it to that module. Do this only when that module is the clear owner.
- If a module changes for several unrelated reasons, split it into one module for each reason. Do this only when the split decreases the coupling or the review cost now.
- If a caller walks a chain through internal structure, for example `a.b().c().d()`, hide the walk behind one operation.
- If inheritance gives a subclass the wrong behavior, use composition or a narrower interface.

### Place a seam

- Add a seam only where behavior varies across it, or where the task needs isolation.
- Before you make a group of modules deeper, classify each dependency. The kind decides where the seam goes and how tests cross it:
  - **In-process:** pure computation or in-memory state, with no I/O. Merge the modules. Test through the new interface. No adapter is necessary.
  - **Local stand-in:** a dependency with a local stand-in for tests, for example an in-memory file system. Test with the stand-in. The seam stays internal, so the interface has no port.
  - **Owned remote:** a service that the project owns, across a network. Put a port at the seam, with an HTTP adapter for production and an in-memory adapter for tests. Then the logic stays in one deep module, although it runs across a network.
  - **External:** a third-party service that the project does not control. Take it through an injected port. Give the tests a mock adapter.

## Self-check

When you add a module, change an interface, or place a seam, answer these questions. Also answer them when you review such a change.

1. The deletion test: if you delete the module and put its code into its callers, does the complexity vanish or spread? If it vanishes, the module was a pass-through: inline it. If it spreads to several callers, the module earns its place. **Evidence:** the command that finds the callers, their count, and the `file:line` of a caller that would repeat the logic.
2. How many adapters does each new seam or port have?
   - One adapter means a hypothetical seam, and two adapters mean a real seam. An adapter for production and one for tests count as two.
   - A seam whose adapters come from external users, for example a plugin interface, is a real seam, also with one adapter in the repository.
   - Add a port only when you can justify two adapters. Remove a hypothetical seam.
   - **Evidence:** each adapter as `file:line`. For a claim of an external user, the published export or the public interface spec as `file:line`.
3. Can you reduce the methods, simplify the parameters, or hide more behind the interface? Does a caller need to know less? **Evidence:** the interface before and after: the count of methods and parameters, and the facts that a caller must know.
4. Does the next likely change touch fewer places? Does the module own each rule that its callers repeated before? **Evidence:** the files that the next likely change touches, before and after. For each such rule, a search outside the module that finds no copy.
5. Can code outside the module import its internals? Try an import from outside. Then remove the trial import as [guard-one-way-doors](guard-one-way-doors.md) says in "Protect the working tree of the user". **Evidence:** the build or lint error for the trial import, or the reason why the language cannot block it.

Then compare the change with each sign below. **Evidence:** for each match, the move from "Pattern" that you made, or the condition of that move that does not hold.

## Signs that you skipped it

The interface hides little:

- Shallow module: callers coordinate several methods for one operation, or options expose internal stages.
- Pass-through method, or Middle Man.
- Many small modules for one concept.
- A seam with one adapter.
- You want to test past the interface, or the code is hard to test through its interface.

Knowledge sits in the wrong module:

- Information leakage: one change needs coordinated edits in several modules, because they depend on one internal decision.
- Temporal decomposition: modules named after stages of execution, such as a loader, a validator, and a saver, repeat one representation and its invariants.
- Feature Envy, Message Chains, Divergent Change, or Refused Bequest.

The interface is wider than it looks:

- Importable internals: code outside the module imports an internal part.
- An internal seam is part of the interface only because tests use it.

## Relations

- [test-behavior-not-implementation](test-behavior-not-implementation.md): it holds where tests go and what they mock. The network call to an owned remote service crosses a system boundary. Thus the in-memory adapter at that seam is a mock at a system boundary, and that principle permits it. After a module becomes deeper, it also holds the deletion of the old tests of the shallow modules.
- [boundary-discipline](boundary-discipline.md): it defines a system boundary. It also holds the leakage of transport, storage, and wire types through an interface, and how to pass an external dependency in.
