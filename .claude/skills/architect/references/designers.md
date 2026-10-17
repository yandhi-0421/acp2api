# Brief for the three designers

Sections: for the main conversation, your task, inputs, do not, your constraint, the format of your design, the vocabulary, the design rules.

## For the main conversation

Start three designers at the same time. Give each the full path of this file and one of the three constraints. Give each the inputs as the section "Inputs" says. Give them neither the conversation nor the other designs.

If no dependency needs a port, replace ports and adapters with another named constraint. Say so in the draft. An example is "the fewest changes to the callers that exist". A seam with one adapter is hypothetical, as [deep-modules](../../principles/references/deep-modules.md) says.

## Your task

The main conversation gives you the path of this file, one constraint, and the inputs below. You make one design. Two other designers make a design at the same time, each with another constraint. You do not see their work.

## Inputs

The main conversation puts these in the prompt:

- the limits of the frame, which are the output of the ground step: what to preserve, change, and avoid, and the risks;
- the FRs in scope;
- the category of each dependency, as "Place a seam" in [deep-modules](../../principles/references/deep-modules.md) defines it;
- your constraint;
- in the improve mode, the chosen opportunity with its pain and its cause.

It gives these as paths:

- the PRD with its list of FRs, and the feature directory;
- `GLOSSARY.md` and the accepted ADRs.

You may read the code of the repository. You do not get the conversation.

## Do not

- Do not change a file. Return text. The main conversation writes the file.
- Do not write an implementation. Give types, signatures, and short pseudocode for tricky logic only.
- Do not move toward a middle design. Apply [design-it-twice](../../principles/references/design-it-twice.md): give your design the best form that your constraint allows. The differences between the designs are the signal.
- Do not add an option, a layer, or an adapter for a need that nobody has. Apply [subtract-before-you-add](../../principles/references/subtract-before-you-add.md).

## Your constraint

Use the one constraint that the main conversation gives you:

- **The smallest interface.** Use one to three entry points at most. Maximize leverage for each entry point.
- **The simplest common call.** Optimize for the most common caller. Make the default case trivial.
- **Ports and adapters.** Put a port at each seam where a dependency crosses and you can name two adapters.

If the main conversation gives you another named constraint, use that constraint.

## The format of your design

Give these parts, in this order:

1. **Usage.** Write this first. Give the code of one to three real callers. Then derive the types from them. If they disagree, change the types, not the usage.
2. **Interface.** Give the types, the methods, and the parameters, with the invariants, the order constraints, and the error modes.
3. **What the implementation hides.** Name the decisions and the work behind the interface.
4. **Seams and adapters.** Give each seam with the category of its dependency, and its adapters. Name the seam where tests cross, and the adapter that the tests use.
5. **Trade-offs.** Say where leverage is high, and where it is thin. Mark each one-way decision, as [guard-one-way-doors](../../principles/references/guard-one-way-doors.md) defines it, and each new dependency. Name each assumption and each product choice that your design needs.
6. **Callers.** If your design replaces an interface that has callers, say how the callers move.

## The vocabulary

Use the words of "Words" in [deep-modules](../../principles/references/deep-modules.md). Use the terms of `GLOSSARY.md` for the domain. Say "design", not "candidate".

## The design rules

For each kind of decision below, read the named principle and apply it.

- A feature that enters code that exists: [redesign-from-first-principles](../../principles/references/redesign-from-first-principles.md).
- A new dependency: [choose-boring-technology](../../principles/references/choose-boring-technology.md).
- The core types and the data structures: [make-illegal-states-unrepresentable](../../principles/references/make-illegal-states-unrepresentable.md).
- The place of validation, the pure core, and the types that the interface exports: [boundary-discipline](../../principles/references/boundary-discipline.md).
- The size of the interface, and the place of each seam: [deep-modules](../../principles/references/deep-modules.md).
- State with two or more writers: [separate-before-serializing-shared-state](../../principles/references/separate-before-serializing-shared-state.md).
- A state change that can run twice or stop half way: [make-operations-idempotent](../../principles/references/make-operations-idempotent.md).
- The path that a reader traces through the files: [minimize-reader-load](../../principles/references/minimize-reader-load.md).

For each principle that changed a decision of your design, name the principle in the part that holds that decision.
