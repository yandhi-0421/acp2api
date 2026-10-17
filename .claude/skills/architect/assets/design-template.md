# [Feature or opportunity] design

## One-way decisions and new dependencies

[Each one-way decision: the decision, its reason, its effect, and its way back. Each new dependency: the facts that choose-boring-technology asks for. If a decision contradicts an accepted ADR, name the ADR. If there are none, write "None", and the reason.]

## Problem

[One paragraph: what the design must do, and why the shape is not obvious. In the feature mode, name the PRD. List each FR in scope with the usage or the module that serves it. List each FR that is not designed, with its blocker. Add the limits of the ground step: preserve, change, avoid, and risk.]

## Pain and cause

[Improve mode only. The pain: what the change costs now. The cause: the decision that causes that cost. Remove this section in the feature mode.]

## Usage

[Write this first. The code of one to three real callers: what they import, what they call, and what comes back. The types of "Shape" derive from this usage. If they disagree, change the types.]

## Shape

[The recommended design:

- data structures first, then the flow of data through the signatures;
- the interface, with its invariants, order constraints, and error modes;
- what the interface hides, what stays open to callers, and why the interface is no larger;
- a text diagram of the structure before and after;
- the principle behind each decision, by name, with no restatement.

A small design is one block. A large design adds a list of its modules.]

## Seams

[Each seam: its place, the category of its dependency, its adapters, and how tests cross it. The test seams belong to the design.]

## Merge decision

[The recommendation, and its main cost. The base design, and why. Each part taken from another design, with its source. Each rejected part, with its reason. Then one score table for each design and one for the merged design.]

| Design red flag | Principle | Result | Evidence | Cost |
|---|---|---|---|---|
| shallow module | deep-modules | [found or absent] | [design or code, and the answer to the self-check] | [cost if found] |
| information leakage | deep-modules | | | |
| temporal decomposition | deep-modules | | | |
| pass-through method | deep-modules | | | |
| importable internals | deep-modules | | | |
| split ownership | separate-before-serializing-shared-state | | | |
| two ways to do one task | migrate-callers-then-delete-legacy-apis | | | |
| hand-synced list | make-illegal-states-unrepresentable | | | |
| depth | deep-modules | [deep or shallow] | [the answers to the self-check] | |

## Trade-offs

[One line for each trade-off: "We accept X in exchange for Y." Name each choice that a later reader can take for an oversight.]

## Other designs

[Give each other design with its interface and one usage example. Say how its structure differs from the recommended design, its main strength, and why it lost. If you skipped the extra designs, give the evidence for the skip that the self-check of design-it-twice asks for.]

## Preparatory refactorings and migration shape

[Preparatory refactorings: the refactorings that make the change easy, and that go before it. Migration shape: how the callers move from the old interface to the new interface. Say whether the two paths exist together, and why. `to-tickets` orders the work.]

## Moves and checks

[Improve mode only. The main moves, and the migration techniques. The checks that keep the behavior. The first vertical slice. Remove this section in the feature mode.]

## Open questions

[Numbered questions, each with a recommended answer, a plain reason, and the evidence. Include each risk and each assumption as a question, so that the answer of the user resolves it. After the approval, write the answer under its question and in the part that it affects. Keep each unanswered question open. If only a trial can settle a question, for example how an interaction feels, name the skill `prototype` in its recommended answer.]
