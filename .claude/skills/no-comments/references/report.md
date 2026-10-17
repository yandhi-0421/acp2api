# The report

Sections: the place, the items.

## The place

Write the report in Chinese. Use the word of the "Chinese term" column of `GLOSSARY.md` for each glossary concept. Write it to `<report dir>/no-comments.md`, as "The run directory" in [scope.md](scope.md) says. If the file exists, add the new report after the old report.

The first line of each report names the branch and the head commit at the end of the run. `code-review` uses a report only for the commits up to its head. The system can clear its temporary directory, so the body of the commit of the deletions also lists each accepted reshape flag.

Before you send the report, apply [protect-secrets-and-personal-data](../../principles/references/protect-secrets-and-personal-data.md). Here it scans the report for secrets and personal data. Then send the report in the chat.

## The items

`code-review` reads each open reshape flag as a lead. Give each fact with its command and its output line, or with its `file:line`. Give these items:

- the scope, the base, the branch, and the head at the start and at the end;
- the files that stayed out, counted by kind;
- the count of deleted comments, of kept comments, and of restored deletions;
- each kept comment, with its exception and its proof;
- each restored deletion, with its exception and its proof;
- each check that failed after the deletions, and the comment that you restored for it;
- each reshape flag that a refactoring closed, with the refactoring and its commit;
- each open reshape flag, with its four fields and its owner;
- each failed report of a subagent, with its failure;
- the failures at the start and at the end;
- the commits, or the reason why the run made no commit;
- the door of the change, from self-check 2 of [guard-one-way-doors](../../principles/references/guard-one-way-doors.md);
- the draft follow-up issues;
- the lessons with their destinations, as [encode-lessons-in-structure](../../principles/references/encode-lessons-in-structure.md) says. Propose text for `CLAUDE.md`, a workflow skill, or a principle. Change none of them;
- each changed glossary entry;
- the report lines of the principles that you read.

Ask once if you may create the draft follow-up issues. In the workflow, then end with the next step: `test-audit`.
