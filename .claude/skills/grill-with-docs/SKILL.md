---
name: grill-with-docs
description: Grill an idea with the user, and record the settled terms and technical decisions in GLOSSARY.md and the ADRs. Use it in place of /grilling in a project that uses the workflow.
disable-model-invocation: true
---

Call the Skill tool for `grilling` and for `domain-modeling`. Follow both skills.

Where the two skills give different instructions, follow these rules:

- Put each conflict and each question of `domain-modeling` into the next round of `grilling`. Do not send any of them in a message of its own, because that splits the round. If no round follows, list each open conflict and question as a deferred point.
- Before you change any file, save the output of `git status` and `git diff` outside the working tree, as [guard-one-way-doors](../principles/references/guard-one-way-doors.md) says.
- A change to `GLOSSARY.md`, `GLOSSARY-MAP.md`, or an ADR records a decision of the user. It is not a plan or an action on the idea. When the user's answer settles a technical decision that meets the three ADR conditions of `domain-modeling`, write the ADR. When an answer has no open conflict or question of `domain-modeling`, make its change at once. Do not wait for the confirmation of the summary.
- Before `grilling` ends, change no other file. In the summary, list each change to another file that `domain-modeling` requires. After `grilling` ends, make each of these changes that the user did not reject.
- In the summary of `grilling`, list each change to `GLOSSARY.md`, to `GLOSSARY-MAP.md`, and to an ADR. For each new ADR, give one short reason for each of its three conditions. The user can then confirm or reject each change. Undo each change that the user rejects. Use an edit that touches only that change, as `guard-one-way-doors` says. A deprecated or superseded status that a change puts on an older entry or ADR is part of that change.
- If the user asks you to start the work before the summary, give the list of changes first. Include the deferred points and the ADR reasons. Then start the work.
- In your final message after `grilling` ends, give the report lines that [principles](../principles/SKILL.md) requires.
- If the user changes an answer before `grilling` ends, replace or remove the change that you made for it. Keep no deprecated or superseded record of that change.
