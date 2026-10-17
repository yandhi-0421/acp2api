# The report

Sections: the place, the items.

## The place

Write the report in Chinese. Use the word of the "Chinese term" column of `GLOSSARY.md` for each glossary concept. Write it to `<report dir>/test-audit.md`, as "The run directory" in [audit.md](audit.md) says. If the file exists, add the new report after the old report. Do not replace the old report, because `code-review` still needs its evidence.

The first line of each report names the branch and the head commit at the end of the audit. `code-review` uses a report only for the commits up to its head. The system can clear its temporary directory, so the commit bodies also hold the evidence of each deletion and the quote of each approval.

Before you send the report, apply [protect-secrets-and-personal-data](../../principles/references/protect-secrets-and-personal-data.md). Here it scans the report and the pasted output for secrets and personal data. Then send the report in the chat.

## The items

`code-review` reads these items as leads. Give each fact with its command and its output line, or with its `file:line`. Give these items:

- the scope, the base, the branch, and the head at the start and at the end;
- the door of the change, from self-check 2 of [guard-one-way-doors](../../principles/references/guard-one-way-doors.md). An example of a one-way decision is a deleted export that an external user calls;
- the failures at the start and at the end;
- the coverage table, the reopened issues, and the scenarios without a slice tag ([coverage.md](coverage.md)), or "no PRD";
- the audit table, with each mark and its evidence line;
- the root cause of the tests that protected nothing, and the categories of the removed tests;
- each test that matched a sign but stays, with the contract that keeps it;
- each deleted or merged test, with the fields of "The evidence of a deletion" in audit.md;
- each added test, with the mutant that it kills and the run that shows it;
- the mutants: the command of the script, and the table that it printed;
- each row whose result you changed after you read its log, with the log line;
- each mutant that survived, with its gap or its evidence of equivalence;
- each line that is `not mutated`;
- each change of the acceptance contract: the question, the quoted answer, the commit, and each new freeze commit;
- each question that waits for the user, with its stop reason;
- each product defect, with its reproduction and its owner;
- the line counts of `git diff --numstat`, with the production code apart, and the net change of the count of tests. If the count grew, say why;
- the draft follow-up issues;
- the lessons with their destinations, as [encode-lessons-in-structure](../../principles/references/encode-lessons-in-structure.md) says. Propose text for `CLAUDE.md`, a workflow skill, or a principle. Change none of them;
- each changed glossary entry;
- the report lines of the principles that you read.

Ask once if you may create the draft follow-up issues. In the workflow, then end with the next step: `code-review`.
