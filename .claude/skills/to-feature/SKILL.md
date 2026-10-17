---
name: to-feature
description: >-
  Write the Gherkin scenarios of a PRD, each tagged with its FR, or revise them when the PRD changes
  or a review or a test run finds a defect in the scenarios. Use for "write the Gherkin for this PRD", "turn these
  requirements into feature files", "update the scenarios".
---

# To Feature

## Goal

Turn the functional requirements (FRs) of a PRD into clear rules, concrete examples, and valid Gherkin scenarios. Keep the product decisions of the PRD. Show each open decision, which is a product choice that no source settles. Do not choose an answer for the user.

Stop at the scenarios. Do not automate them, freeze them, or split them into issues.

## Steps

1. **Read.** Read the PRD with its status, the feature files that exist, the configuration of the Gherkin runner, and `GLOSSARY.md`. Read [document-map.md](../domain-modeling/references/document-map.md) and [ste.md](../domain-modeling/references/ste.md). This step is complete when you can name the PRD, the feature directory, and the FRs in scope.
2. **Split.** For each FR, find the actors, outcomes, rules, constraints, and exclusions. Split broad input into independent, end-to-end behaviors by observable outcome. For broad, stateful, or ambiguous input, or for boundary values, read [discovery-and-coverage.md](references/discovery-and-coverage.md). This step is complete when each FR in scope has its behaviors, and each behavior gives one observable outcome.
3. **Map.** For each behavior, map the rule to its equivalence partitions, its boundary values, and one concrete example for each. Write each unresolved claim as an open decision. If every answer to a gap gives the same result, map the example. Note the gap in the report. The example map is a note, not a deliverable. This step is complete when each equivalence partition and boundary value has one of these:
   - an example;
   - for an equivalence partition, the same result as a named example;
   - a source-backed exclusion;
   - an open decision that blocks it.
4. **Write.** Read [gherkin.md](references/gherkin.md). Write one `.feature` file for each capability, in the directory that the document map gives. Translate the settled examples, not the paragraphs of the PRD. Give each scenario its FR tag. This step is complete when each example is a scenario in a file, or has a stated reason.
5. **Check.** Run the checks below. Then validate the syntax as `gherkin.md` says. Repair each failed check. Run the checks again.

To revise, run these steps on the files that exist. Also follow "Revise scenarios" in `references/gherkin.md`.

## Completion conditions

- Each FR in scope has a scenario with its FR tag, or the report gives the reason. Run the coverage commands of "FR tags" in `gherkin.md`.
- Each scenario maps to a rule of the example map and to an observable result.
- Interface check: no step describes navigation or names a control, a page, a selector, a private call, or a database table. The exception is an FR that is about the interface.
- Duplicate check: each scenario and each outline row matches one equivalence partition or one boundary value of the example map. No two match the same one.
- Fail check: ask of each scenario which close wrong implementation fails it, for example a wrong limit or a wrong value. Name one. A `Then` that says only that no error occurs, that the action works, or that a value appears fails this check.
- The parser ran, or the report says that parser validation is unverified.

## Constraints

- Use only the behavior, the rule values, and the limits that the PRD states. A rule value that a scenario needs and the PRD lacks is an open decision. Examples are a threshold, a rate, a time limit, and a message. You can choose input data whose result the stated rules fix. An example is 100 EUR for a limit of 500 EUR. Do not present an open decision as confirmed behavior.
- This skill never edits the PRD. Only `to-prd` revises it. The PRD keeps the product decisions, so a decision that only a scenario holds is a second copy.
- If the PRD conflicts with the code, the glossary, or an ADR, report the conflict. If you know which source decides, name it.

## Output

The report is in Chinese. It lists:

- the feature files, and the count of scenarios for each FR;
- each FR or example without a scenario, with its reason;
- each open decision, and each source conflict;
- each gap that you noted in step 3;
- the parser status;
- the report lines that [principles](../principles/SKILL.md) requires for each principle that you read.

## Stop rules

- If no PRD exists, tell the user. Suggest `to-prd`. Write nothing.
- If the PRD is a `Blocked draft`, keep all blockers in the example map as open decisions.
- If a product choice changes a rule, an example, or a result, halt only that behavior. Use `grilling`. After the user confirms the summary of `grilling`, load `to-prd` to revise the PRD. Then continue. A choice that the user defers, or that needs new evidence, stays an open decision.
- If a `bdd` run froze a scenario, ask the user before you change it. Name stop reason 1 of [no-stops-after-approval](../principles/references/no-stops-after-approval.md). That principle also covers each question in a run without a user.
- At the end, ask the user to confirm the scenarios. Start no later skill.
