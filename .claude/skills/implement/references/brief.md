# The brief of the subagent

Sections: the form, the fields, the approval, the fences, the report.

## The form

Write one brief file for each issue in `<run>`, outside the repository. Then the subagent cannot change it. Name the file `brief-<n>.md`. The brief gives pointers only. Do not copy the issue, a scenario, the written design, or a principle into it. A copy goes out of date.

The word "only" limits the input of the task. The base context of the subagent comes from `CLAUDE.md`: the principle index, `GLOSSARY.md`, and the ADRs. If `CLAUDE.md` does not tell the subagent to read them, add their paths to the brief. The principle index has `disable-model-invocation`, so name its path, for example `.claude/skills/principles/SKILL.md`. Name no model, because the subagent uses the model of this session. Keep secrets out of the brief.

## The fields

- **Issue.** The number of the issue. The subagent reads it with `gh issue view <n>`.
- **Kind.** A slice issue has the heading `## Scenarios`. A refactoring issue has the heading `## What to change`.
- **Recipe.** For a slice issue, the path of `bdd/SKILL.md` in the skills directory of the project, for example `.claude/skills/bdd/SKILL.md`. The subagent reads it as a file, because it may lack the Skill tool. For a refactoring issue, the path of `references/refactoring.md` in the directory of `implement`, for example `.claude/skills/implement/references/refactoring.md`. For both kinds, name the sections "The signal" and "The signs" of `.claude/skills/implement/references/wrong-design.md`. Also name `.claude/skills/domain-modeling/SKILL.md`, because the subagent may lack the Skill tool.
- **Branch.** The issue branch, which is the current branch. The subagent commits on it and on no other branch. If the branch has a commit that marks earlier work, say so, because the subagent then resumes. A commit marks earlier work when its first line ends with `(#<n>)`, or is "Save the unfinished work of issue #<n>".
- **Design.** The path of the written design, and the headings that the issue names. The approval of the design confirms its seams, so the subagent asks no question about them. If no written design exists, say so.
- **Done issues.** The numbers of the done slice issues, so that `bdd` checks their freezes too.
- **Check.** The heading "Done when" of the issue, and the path of `<run>/start.txt`. For a retry, also the path of `<run>/failed-<n>.txt`. Only when the issue names a failure at the start, the subagent fixes it.
- **Approval.** See the next section.
- **Fences.** See the section "The fences".
- **Report.** See the section "The report".

## The approval

Name the approval in the brief, because the brief carries no approval without that name. Name each of these that exists:

- the written design that the user approved, with its path;
- the issue breakdown that the user confirmed;
- the request of the user for this run.

State that the approval covers this issue only. If the user answered a stop reason, add the answer and its stop reason. Tell the subagent that it works without a user, and that it follows "A run without a user" in [no-stops-after-approval](../../principles/references/no-stops-after-approval.md).

## The fences

The subagent changes none of these:

- the lines of the files of the acceptance contract of each done slice issue. Its freeze commit lists the files, as the section "The freeze commit" of [freeze.md](../../bdd/references/freeze.md) says. The subagent may add a line that no frozen scenario uses, and may make the change that the approval in the brief names;
- the scenarios of other issues, and the scenarios without a slice tag. In a scenario of this issue, the subagent may remove `@wip` and nothing else;
- the PRD and the written design;
- the brief, and the files in `<run>`, except its worktree;
- `CLAUDE.md`, the workflow skills, and the principles. The subagent proposes text for them in the report;
- `GLOSSARY.md` and the ADRs, except through `domain-modeling`;
- the remote and the issue tracker. The subagent does not push, open a pull request, close an issue, or write a comment.

Only when the result changes a durable decision or a published contract, update a document. Use the document that [the document map](../../domain-modeling/references/document-map.md) names for it. The fences above still hold. After the build, the code and its tests hold a decision about an interface. Put this paragraph in the field Fences.

At the build step, a conflict with the glossary is an agent decision in the report. The subagent does not wait for an answer. Put this paragraph in the field Fences.

## The report

The subagent reports in the form of the Output section of `bdd`. For a refactoring issue, it uses the Output section of `refactoring.md`. The report must also give these facts to `implement` and to `code-review`:

- each question with its stop reason;
- each agent decision with its reason, and each draft follow-up issue;
- for each red test, the command and the failure line;
- the path and the command of each lever;
- the report lines of the principles that the subagent read.
