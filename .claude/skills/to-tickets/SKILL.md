---
name: to-tickets
description: >-
  Slice a PRD, its Gherkin scenarios, and the written design into GitHub issues, one vertical slice each, with blocking edges. Tag each scenario with its issue. Use for "slice the work into issues", "break this design into issues".
---

# To Tickets

## Goal

Turn the PRD, its feature files, and the written design into GitHub issues. Make one issue for each vertical slice, with its blocking edges, under one parent issue. Tag each scenario with its issue, so that `implement` and `bdd` find the scenarios of one issue.

## Steps

At each step that names a principle, apply it there. Follow the report rule of [principles](../principles/SKILL.md).

1. **Read.** Read [document-map.md](../domain-modeling/references/document-map.md), [ste.md](../domain-modeling/references/ste.md), `GLOSSARY.md`, the accepted ADRs, and `docs/agents/issue-tracker.md`. Read the PRD with its status, the feature files, and the written design. If the written design has the section "Moves and checks", it is the improve mode. Check the stop rules for absent documents. This step is complete when you can name the written design, or say that none exists. In the feature mode, name the PRD of the request, its status, and each scenario that has a slice tag.
2. **Slice.** Read [slicing.md](references/slicing.md). Put each scenario in scope into exactly one vertical slice. Apply [sequence-verifiable-units](../principles/references/sequence-verifiable-units.md). Here it changes the slice boundary: no issue needs a later issue to repair a new failure. For a migration, apply migrate-callers-then-delete-legacy-apis as slicing.md says. This step is complete when each scenario or move in scope is in one slice or in the list of scenarios not sliced.
3. **Order.** Add each blocking edge as "Blocking edges" in slicing.md says. This step is complete when each edge has a reason, the cycle check of slicing.md passed, and you can name the first frontier.
4. **Draft.** Read [publish.md](references/publish.md). Fill [issue-template.md](assets/issue-template.md) for the parent issue and for each issue, in files outside the repository. Label each sub-issue `ready-for-agent`. If its body names a decision or a check of the user, use `ready-for-human` instead. Then do each check of "Before the stop".

   Give each issue a title and a body. Give each sub-issue its label and edges. This step is complete when these exist and each check has its output.
5. **Stop.** Show the breakdown in the form of the template, and the full text of each body. Give a recommended answer and a plain reason for each choice.

   Apply [guard-one-way-doors](../principles/references/guard-one-way-doors.md). Here it changes one thing: an issue is an external publication, so the approval of the breakdown does not confirm it.

   Then stop. If the user asks for a change, revise the drafts. Then show the drafts again, also when the message names the publication. If the user only approves the breakdown, ask once for the publication. This step is complete when the words of the user name the publication after the last drafts that you showed. They can be in any language, for example "publish".
6. **Publish and tag.** Apply make-operations-idempotent and "The details of the order" of publish.md. If the parent issue is absent, create it. Then create the issues in dependency order. Finish each issue before the next issue. For one issue, the order is: label, sub-issue link, blocking edges, and then the tags of its scenarios. This step is complete when the parent issue exists, each sub-issue has its edges and its label, and its scenarios have their tags.
7. **Prove.** Apply [prove-it-works](../principles/references/prove-it-works.md). Here it changes one thing: a change in the issue tracker is unproven until you read it back. Run the read-back and the tag checks of publish.md. Repair each failure. Then run the failed check again. This step is complete when each check has its output, and no failure remains.

## Completion conditions

- The parent issue has all other issues as sub-issues.
- In the feature mode, each scenario in scope has exactly one slice tag, or the report lists it as not sliced.
- In the feature mode, the tags cause no new failure of the four FR coverage commands of gherkin.md.

## Constraints

- **Tags only.** Add tags to the tag line of a scenario, and change nothing else. Do not commit. Do not change the PRD, the text of a scenario, or the written design. If an issue needs a new domain term or an ADR, load `domain-modeling`.
- **Parent issue.** Add only sub-issues to the parent issue. If the user does not confirm the recovery step, do not close an issue. Do not change a closed issue.
- **Follow-ups.** Write each problem outside the slices as a draft follow-up issue in the report. Create none: that is stop reason 2 of no-stops-after-approval.

## Output

Write the report in Chinese. List the parent issue with its number and title, and each issue with its number, title, label, and blocking edges. Give the first frontier, the path of `<drafts>`, and each scenario not sliced with its reason. In the feature mode, list the files that the tags changed, which need a commit before `implement` starts. Give the open decisions that keep scenarios out of the slices, the draft follow-up issues, and each changed glossary entry. Add the report lines of the principles that you read.

## Stop rules

- If neither a PRD nor a written design exists, stop. Suggest `to-prd`.
- If the PRD has no feature files, stop. Suggest `to-feature`.
- If `docs/agents/issue-tracker.md` does not exist, stop. Tell the user to run `/workflow-setup`.
- If a PRD has no written design, do not stop for it alone. Say so in the first line of step 5, and recommend `architect`.
- If no scenario and no move is left to slice, create no issue. Still add each absent `@wip` as publish.md says. Then run the tag checks of step 7.
- If you did not see a `prd-review` report, say so in the report. Continue.
- For a `Blocked draft` or an FR that is not designed, follow "Scenarios not sliced" in slicing.md.
- Apart from the stop rules above and in slicing.md, stop only at step 5. Its question about the publication is part of it.
- Start no later skill.
