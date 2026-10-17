---
name: architect
description: >-
  Design the module interfaces and structure of a feature after its PRD. Do the same for code that is hard to change or test. Compare designs. Recommend one. Use for "design the structure for this feature", "this module is hard to test", "find deeper modules".
---

# Architect

## Goal

The skill makes one written design of a change. `to-tickets` and `implement` read the design that the user approved.

The feature mode starts from a PRD. The improve mode starts from friction in code that exists, or from a request for a deeper module. In the improve mode, read [improve.md](references/improve.md) first.

The user cannot judge the technical decisions. For each decision, give a recommended answer, a plain reason, and the evidence.

## Steps

At each step that names a principle, apply it there. Follow the report rule of [principles](../principles/SKILL.md).

1. **Ground.** Read [document-map.md](../domain-modeling/references/document-map.md), [ste.md](../domain-modeling/references/ste.md), `GLOSSARY.md`, the accepted ADRs, and the written designs that exist for this feature or area. In the feature mode, also read the PRD with its status and the feature files with their FR tags. Before you trace the code, read [ground.md](references/ground.md). Apply [prove-it-works](../principles/references/prove-it-works.md): a claim about the reason for code is unproven until a source supports it. This step is complete when you can explain these, with a `file:line` for each fact:
   - why the change is costly;
   - where the code splits one body of knowledge;
   - which contracts it can affect;
   - which checks protect the behavior.

   Only if no code that exists connects to the new code, skip the trace. Say so in the frame. Then this step is complete.
2. **Frame.**
   - Write the limits of the change and each dependency with its category. Add a rough sketch that makes the limits concrete, not a proposal. Apply [deep-modules](../principles/references/deep-modules.md): the category of a dependency decides where its seam goes.
   - Show the frame to the user. Go on at once, because the user reads while the designers work.
   - If an exception in the self-check of [design-it-twice](../principles/references/design-it-twice.md) applies, skip step 3. Write one short design with the evidence for the skip. Go to step 4.

   This step is complete when the user has seen the frame, and you wrote the decision on design-it-twice in the frame.
3. **Design.** Read [designers.md](references/designers.md). Start three subagents at the same time, as designers.md says. Apply design-it-twice: the designs must differ in structure, not in names. If a pair differs only in names, start one designer of that pair again, with another named constraint. Do this once. This step is complete when three designs have returned with all parts that designers.md lists.
4. **Score and merge.**
   - Read each design in full. Fill the score table of [design-template.md](assets/design-template.md) for each design.
   - The base is the design with the fewest design red flags found. A tie goes to the smaller interface. A found red flag is a reason to revise the base or to take a part from another design.
   - If the designs solve different problems, return to step 2 once. If the designs solve different problems again, put the problem of each design in the open questions. Recommend the design that fits the frame.
   - If the designs still differ only in names, use the common shape.
   - Apply subtract-before-you-add: the merged design adds nothing that nobody needs. Apply guard-one-way-doors and choose-boring-technology as the top of the template says.
   - Fill the score table of the merged design. Write the draft with the template, in English as ste.md says, in a temporary directory outside the repository. If the project has a type checker, write the types of the Shape section as stubs there. Compile the usage examples against them.

   This step is complete when the draft has every part of the template and one recommendation.
5. **Approve and write.** Show the draft. State that one model made all the designs, so their agreement is weak evidence. Put these parts first, in this order:
   - the one-way decisions and the new dependencies, each with its reason;
   - the recommended design with its usage;
   - the score tables, and the seams with the test seams, so that `tdd` does not ask again;
   - the numbered open questions, each with a recommended answer.

   Ask the user to approve the design. Then stop. Under guard-one-way-doors, the approval confirms the one-way decisions at the top and no other one-way action. Edit each part that a small change affects. A change or an answer is small only if it keeps every interface, seam, owner of state, dependency, and one-way decision. Otherwise, run steps 2 to 5 again.

   After the approval, put the answer of the user to each open question into the part that it affects. Write the design to `docs/design/<name>.md`. In the feature mode, `<name>` is the file name of the PRD without the extension. In the improve mode, it is a short name of the opportunity. If `docs/design/` is new, update `CLAUDE.md` as the document map says. This step is complete when the file holds the approved design.

If the user adds a requirement that the PRD lacks, ask the user to update the PRD with `to-prd` first. Then run steps 1 to 5 again. If the user adds a limit or evidence, or `implement` proves the design wrong, ground again as `ground.md` says. Then run steps 2 to 5 again.

## Completion conditions

- A reader who did not see the conversation can build from the written design. It names the interfaces, the seams, and the one-way decisions that `implement` checks.
- Each FR in scope has a design that serves it, or the report lists it as not designed.
- The user approved the design.

## Constraints

- **Design only.** Write no code and no scaffold in the repository. Change no repository file before the approval, `GLOSSARY.md` and the ADRs included. A probe or a check in a temporary directory outside the repository is allowed.
- **One model.** All subagents use the model of this session, so their agreement is weak evidence. Prove an important claim with code that runs, in a probe that changes no external state. Do not choose another model for a subagent.
- **Words.** Say "designs", not "candidates". Say "approve" for a design, and "confirm" only for a one-way action or decision.
- **Documents.** If the design needs a new domain term or an ADR, put it in the open questions of step 5. After the approval, load `domain-modeling` to write each glossary entry and ADR that the user accepted. Only when the three ADR conditions of `domain-modeling` hold, offer an ADR.
- **Product choices.** Only `to-prd` writes the PRD. Put a product choice that the design needs in the open questions. When the user answers that choice, ask the user to update the PRD with `to-prd` first.

## Output

Write the report in Chinese. Give the path of the written design, its one-way decisions, and its new dependencies. Give each FR that is not designed. Give the open questions that remain. Name each changed glossary entry. Add the report lines that [principles](../principles/SKILL.md) requires for each principle that you read.

## Stop rules

- If no PRD exists and the user asks for a feature design, suggest `to-prd`. Write nothing. If the user names friction in code that exists, use the improve mode.
- If the PRD is a `Blocked draft`, design the FRs that no blocker affects.
- If you did not see a `prd-review` report, say so in the report. Continue.
- In the improve mode, after the list of opportunities, propose no interface. Ask which opportunity to explore. Stop.
- Start no later skill.
