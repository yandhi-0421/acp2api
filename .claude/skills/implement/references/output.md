# The output

Sections: the items, the facts of each issue.

Write the report in Chinese. Use the word of the "Chinese term" column of `GLOSSARY.md` for each glossary concept. The report is for the user, and for the later skill `code-review`. Write the report to `<report file>`, as commands.md defines it. Scan the file as "The secret scan" in commands.md says. Then send it.

## The items

Give these items:

- the parent issue;
- the integration branch, its base ("The base" in commands.md gives the commands), the branch on which the run started, and the run mode;
- each reopened issue;
- the files of the plan commit;
- the issues that the final pull request closes. These are the done issues. Only when all sub-issues are done, add the parent issue;
- the door of the change: two-way, because all work is local commits on new branches;
- each issue left out, with its reason: failed, blocked by an issue, `ready-for-human`, of an unknown kind, or behind an `unverified` edge;
- each edge that only a text edge shows;
- for each failed issue, its cause and the smallest next action;
- each scenario without a slice tag, because it is in no issue. For a PRD with the status `Blocked draft`, say that the blockers keep them out;
- the failures at the start, from `<run>/start.txt`, and at the end;
- the proof that the run made no push and no pull request, from "No push" in commands.md, and the three parts that stay "unproven";
- the agent decisions, the questions with their stop reasons and the quoted answers of the user, and the draft follow-up issues;
- the secret scan, with its command and its result line, and each hit that is not a secret;
- the lessons with their destinations, as [encode-lessons-in-structure](../../principles/references/encode-lessons-in-structure.md) says. Propose text for `CLAUDE.md`, a workflow skill, or a principle. Change none of them;
- each changed glossary entry;
- the report lines of the principles that you read.

Write each draft follow-up issue and each proposed text in English. Report a new failure at the end of the run. Do not fix it.

## The facts of each issue

For each done issue, give a table row or a short block with these facts:

- its merge commit, and, for a slice issue, its freeze commit;
- its units, and the proof level of each check that you ran yourself;
- the red run: the command and the failure line;
- the path and the command of each lever;
- its agent decisions.

Mark each fact that only the subagent gives, such as the red run, as "reported by the subagent". Mark each other fact that you did not prove as "unproven". The skill `code-review` uses the red runs and the levers.
