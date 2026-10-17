# The types of decision issues

Sections: the user and the agent, grilling, prototype, research, task, the resolution comment.

Each decision issue has one label for its type: `wayfinder:grilling`, `wayfinder:prototype`, `wayfinder:research`, or `wayfinder:task`. The type decides how a session resolves the issue. If you are not sure of the type, use grilling.

## The user and the agent

A grilling issue and a prototype issue need the user. Only a live exchange with the user resolves them. Never give the answer of the user yourself. A model that answers its own questions in a grilling has broken this rule, because the user then sees a decision that nobody made.

If the user is absent, stop and wait. Exception: in this session, the words of the user give the decision to you, for example "decide yourself".

If `grilling` gave a recommended answer, use it. Mark it "assumed under the delegation of the user", and quote the words in their language. If `grilling` gave no recommended answer, do not choose. List the question as a deferred point. The user can change each assumed answer later. The exception does not cover a prototype issue: its verdict stays with the user.

Under a delegation, write no ADR and no glossary entry for an assumed answer. List it in the summary.

A research issue is for the agent alone. A task issue is for the agent alone when the agent can do the work. Else it needs the user.

## Grilling

Use it for a decision that a conversation with the user settles. It is the default type.

Read [grill-with-docs](../../grill-with-docs/SKILL.md), and follow it. The question of the issue is the subject of the grilling. The answer is complete when the user confirms the summary of `grilling`.

If the question of the issue stays a deferred point, the issue has no answer. Keep it open. Remove your claim: `gh issue edit <n> --remove-assignee @me`. Report the point.

The resolution comment gives the summary. It names each changed file, for example `GLOSSARY.md` or an ADR, and says if the change is uncommitted.

## Prototype

Use it when the key question is how something must look or behave, and a description cannot show it. The prototype is logic code or UI code. An outline or a draft text is a grilling issue.

Load the skill `prototype`. The question of the issue is the question of the prototype. The answer is complete when the user gives a verdict. The resolution comment gives the verdict, its question, and the branch and the commit of the prototype, or its directory. Unless the user agreed to a push, it says that the branch is local. The prototype itself is not the answer, and it does not go into the map.

## Research

Use it when a decision waits for a fact outside the repository. Examples are documentation, an external API, and a source on the web.

Claim each research issue of the frontier, as tracker.md says. In the work mode, claim them at step 2, together with the issue of the session. The request to work on the map confirms these claims. Then start one subagent for each of them, all at the same time. Give each subagent this brief:

- Load the skill `research`, and answer the question of the issue. Here is the question, and the decision that waits for it.
- Start no subagent. Use at most `<budget>` searches. In the note, list each search that the budget left out.
- Write the note in English. Give the full note in your reply. Write no file. Change nothing in the repository or on the issue tracker.

Replace `<budget>` with a number that fits the question, for example 10. Without its own subagents, `research` takes its path without subagents, and its note says so. This keeps the cost of a chart small. The answer is complete when the note gives the evidence level of each statement, and each gap.

The answer of a research issue is the fact, not the decision. If the note recommends an option, draft a new body of the issue that waits for the fact. Its section "Read" names the research issue and the recommendation. The user decides there.

The note is the section "Reasons and evidence" of the resolution comment. If the comment is longer than 60,000 characters, split it into parts, each with "Part n of m". Write each part to its own file. If the key fact is Unknown, still close the issue: the searches and the gap are the answer. Draft the gap as an item of "Not yet specified", or as a task issue that gets the access. Do not try again in the same session.

After the last subagent returns, do steps 4 and 5 of the work mode for all research issues together. One yes covers all their writes.

## Task

Use it for work that must occur before a decision is possible, and that decides nothing. Examples are access to a service, an account for a test of an API, or a move of data to see its shape. A task issue unblocks a decision. It does not make a part of the destination.

If you can do the work, do it. Apply guard-one-way-doors to each one-way action in it. If only the user can do the work, give a precise checklist. Each step names the place, the action, and the value that the user brings back. For a secret, the step names the file where the user puts it. The user brings back only that the step is done.

The answer is complete when the work is done. The resolution comment gives what was done, and each fact that a later issue needs, for example a URL or a count of rows. For a secret, give only its location, for example "in `.env` as `PAYMENT_KEY`".

## The resolution comment

Write it in English, as [ste.md](../../domain-modeling/references/ste.md) says. Use this form:

```markdown
## Answer

<!-- The decision or the fact, in one to three sentences. -->

## Reasons and evidence

<!-- The summary of the grilling, the verdict of the prototype, the research note, or the record of the task. -->

## Changes to the map

<!-- Each change of step 4 of the work mode, with the exact title of each new issue, or "no change". -->
```

The line in "Decisions so far" gives the answer in one line, with the title and the link of the issue.
