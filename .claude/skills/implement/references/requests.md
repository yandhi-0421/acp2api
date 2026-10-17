# The request

Sections: the key, the request forms, the improve mode.

## The key

The request names the PRD, the written design, the parent issue, or a sub-issue. Take the key from the name:

- For a PRD, the key is its file name without the extension.
- For a written design, the key is its file name without the extension.
- For a parent issue, the key is its title.
- For a sub-issue, its `## Parent` heading gives the parent issue. The key is the title of that issue.

If the request names none of these and the project has one PRD, use it. If it has no PRD and one written design, use that design. [The document map](../../domain-modeling/references/document-map.md) says where they are. If several PRDs have an open sub-issue and the request names none, ask the user which PRD to use. Name stop reason 3.

If the request names a parent issue or a sub-issue, read the parent issue with `gh issue view <parent>`. Otherwise find it as "Read the issues" in commands.md says. Its body names `<path>`, `<design path>`, and `<dir>`.

## The request forms

- A request for the parent issue, or for no issue, means the whole PRD.
- A request for one issue means that issue alone. It overrides the label `ready-for-human`, because the user chose the issue. If an edge into the issue comes from an issue that is not done, report the edge and stop. Name stop reason 3. The report needs no reason for the other sub-issues. "The frontier" in commands.md holds the rule.
- A request for two issues or more means those issues. "The frontier" in commands.md holds the order and the other rules.
- A question that asks only which issue is next gets the plan of step 1, and no build.

## The improve mode

If the parent issue names a written design and no PRD, the run is in the improve mode. The key is the file name of the written design. There is no feature directory, so `<dir>` has no value. Skip the PRD, `<dir>`, and each command that reads scenarios, but not the command that lists the merged issues. Each issue is a refactoring issue. The skill `architect` has the improve mode.

Work a PRD with the status `Blocked draft` as any other PRD. Its blockers keep some scenarios out of the issues. Give those scenarios in the report.
