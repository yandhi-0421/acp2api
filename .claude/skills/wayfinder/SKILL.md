---
name: wayfinder
description: >-
  Plan an effort that is too large for one session as a map issue with decision issues on GitHub. Resolve one decision issue in each session until nothing is left to decide. Use "/wayfinder" with an idea to chart a map, or with a map to work on it.
disable-model-invocation: true
---

# Wayfinder

## Goal

Some efforts are too large for one session, and the way to their end is not clear yet. If one session plans all of it, the context fills. Then the model reasons badly, the compression loses decisions, and the work stops. This skill keeps the plan on the issue tracker, so that each session starts from the same state.

The destination ends the effort: a decision, or a document of the workflow, for example a PRD, a written design, or an ADR. The map is the issue that indexes the effort. Each decision issue holds one question. The frontier is the open decision issues that a session can take now.

Plan only, and write no production code. The map is complete when no decision issue is open, and "Not yet specified" is empty. If only a build remains, you are at the end of the map. Then report, and name the skill that makes the destination. A build goes through `to-tickets` and `implement`, because they hold the freeze and the checks.

The steps use three references: [map.md](references/map.md), [types.md](references/types.md), and [tracker.md](references/tracker.md). Read each one before the first step that names it.

## Steps

At each step that names a principle, apply it there. Follow the report rule of [principles](../principles/SKILL.md). If the user gives an idea, chart a map. If the user gives a map and no new idea, work on it.

### Chart a map

1. **Read.** Read `docs/agents/issue-tracker.md`, [document-map.md](../domain-modeling/references/document-map.md), [ste.md](../domain-modeling/references/ste.md), `GLOSSARY.md`, and the accepted ADRs. Do the checks of "Before a chart" in tracker.md. This step is complete when the labels exist, and you can name the earlier map of this effort or say that none exists.
2. **Destination.** Read [grill-with-docs](../grill-with-docs/SKILL.md), and follow it. Settle the destination first, because it fixes the scope of each question. This step is complete when the user confirms the summary of `grilling` with the destination.
3. **Survey.** Grill again, breadth-first. Ask to find the open questions of the whole effort, not to settle them. Sort each one as map.md says. If no map exists, and no question needs a later session, make no map. Recommend the next skill, and stop. This step is complete when each open question is a draft decision issue, or an item of "Not yet specified" or "Out of scope".
4. **Draft.** Fill [map-template.md](assets/map-template.md) and [issue-template.md](assets/issue-template.md) in a new directory outside the repository. Apply [protect-secrets-and-personal-data](../principles/references/protect-secrets-and-personal-data.md). Here it changes one thing: scan each draft, because an issue is a publication. This step is complete when each draft has its type from types.md and its blocking edges, and the scan has no hit.
5. **Confirm.** Show the destination, the map body, and the full text of each decision issue. Give its type and its blocking edges. Also list each other write of the chart, for example the claims of step 7.

   Apply [guard-one-way-doors](../principles/references/guard-one-way-doors.md). Here it changes one thing: one yes covers each write of the list that you showed, and no other write. Stop. This step is complete when the words of the user confirm the publication after the last drafts that you showed.
6. **Publish.** Publish as tracker.md says. Apply make-operations-idempotent: a rerun does not create a published issue again. Apply [prove-it-works](../principles/references/prove-it-works.md): a write is unproven until you read it back. This step is complete when the read-back matches the confirmed drafts.
7. **Research.** Resolve the research issues of the frontier after step 6, as types.md says. Then stop. A chart resolves no other issue.

### Work on a map

1. **Load.** Read the map body and the list of its decision issues, not each body. Finish each unfinished record, and find the frontier, as tracker.md says. This step is complete when each unfinished record has its writes or a "no" of the user, and you can name the frontier. The frontier can be empty or unknown.
2. **Claim.** Use the issue that the user names. Else take the first issue of the frontier. Claim it before other work, as tracker.md says. The request to work on the map confirms the claim. This step is complete when the read-back shows the claim.
3. **Resolve.** Resolve the issue as types.md says for its type. Never answer for the user in an issue that needs the user. This step is complete when the answer meets the condition of its type.
4. **Update.** Draft the changes that the answer causes, as map.md says. This step is complete when each open issue and each item of "Not yet specified" that the answer touches has a draft or "no change".
5. **Record.** Scan the resolution comment and the drafts of step 4, as step 4 of the chart says. Show them with the line for "Decisions so far". Apply guard-one-way-doors as step 5 of the chart says. After the yes, record them as tracker.md says. Apply separate-before-serializing-shared-state there, because other sessions can edit the map. This step is complete when the read-back shows each write.

Resolve one decision issue in each session. Exception: you can also resolve the research issues of the frontier, as types.md says. If the context fills before a record or a publication, record no half answer. Keep the claim. Write a handoff file with the skill `handoff`.

## Constraints

- **One writer.** Only the main session writes to the issue tracker. A subagent gives its result back.
- **Names.** Name each issue by its title, with the link inside the title, never by a bare number.

## Output

Write the report in Chinese. Give the destination, and the map with its title and link. Name each issue that the session created, resolved, or closed.

Give the frontier, and the next issue or the skill that makes the destination. If the destination is a decision, give it. Name each changed glossary entry and ADR. Add the report lines of the principles that you read.

## Stop rules

- If `docs/agents/issue-tracker.md` or a label of tracker.md is absent, stop. Tell the user to run `/workflow-setup`, because it creates both.
- If the user gives a map and no new idea, and the map is closed, report it, and stop.
- If the user changes the destination of a published map, stop, as map.md says.
- If the map is complete, report it, and recommend the skill that makes the destination. Only after a yes, close the map.
- If no decision issue is open, but "Not yet specified" has items, do steps 3 to 7 of the chart.
- If the frontier is empty or unknown, open issues remain, and the user named no issue, stop. Report the claims and the blocking edges, or the error.
- Start no later skill. Only the skills of types.md and `handoff` run inside this skill. They change files as they say.
