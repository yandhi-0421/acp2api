# Decide and report

Sections: the decision, the report, the place, the owners.

## The decision

Lead with the merge decision. Its three values and their rules are in `SKILL.md`, step 5. A group with no finding does not offset a group with a finding: a change can pass one view and fail another.

- When an unread path or a skipped check can change the decision, do not claim merge readiness.
- When another intent source defines the intent, an absent PRD alone does not change the decision.
- `MERGE` means "ready for `pr`", or for a pull request or uncommitted work, ready to merge or to commit. A merge is a one-way door, so it needs the confirmation of the user.

## The report

Write the report in Chinese. Keep the values of the decision, the severity, and the proof in English, as code text. Use this header:

```text
Pre-merge review: N findings
Recommendation: MERGE | BLOCK | NEEDS DECISION, with the reason
Scope: the kind of change, the target tip, the diff base, the head, and the intended result
```

Then give these parts, in this order:

1. **Each `NEEDS DECISION` point** as a numbered question. Give a recommended answer, its reason, and the effect of each choice on the product or on the risk. The user decides about the product and the risk, not about the code. For an unresolved `MEDIUM` finding, the usual recommendation is a fix before `pr`.
2. **The findings, in three groups.** Start with one line for each view: its count of kept findings, and its worst finding or `No findings`. Then show each group that is not empty: "Project rules and design", "Requirements", and "Engineering risk". Put each finding in the group of its primary violation. Order a group by severity and then by proof.

   Use the finding format of [briefs.md](briefs.md), with the line `Proof: confirmed | probable` after the title. In the field "Evidence", also cite the other rules or requirements that the finding breaks. Give each finding its owner (see "The owners").
3. **The dropped findings**, each with its reason.
4. **Coverage and validation**: the changed paths, the traced behavior, and the test gaps. Also the commands with their results, the contract check, the reported facts and quotes, and each skipped check with its reason.
5. **The result of `blast-radius`**, or the reason why it was not called. Give the door of the change and its blast radius in one word, for `pr`.
6. **`Unknowns`**: the facts that can change a finding or the decision.
7. **The merge gates of the repository**, for example status checks or required reviews, if they are available. They are not defects of the code.
8. **Outside the scope**: each unrelated old defect that a view saw, as a draft follow-up issue.
9. **The limit**: the views use one model, so their agreement is weak evidence. No evaluation has verified this skill.
10. **The report lines** of the principles, as the report rule of [principles](../../principles/SKILL.md) says. Give them although the review changes no file. Before you send the report, apply [protect-secrets-and-personal-data](../../principles/references/protect-secrets-and-personal-data.md). Here it scans the report for secrets and personal data.

If no finding stays, write `No findings`. Keep the scope, the checks, and the limits that can change the decision. If the scope is empty, write `No changes in scope`, and omit the count and the decision.

## The place

Before you send the report in the chat, write it to the file `code-review.md`. Then `pr` can read it in a later session. In the workflow, the file is in the run directory of `implement`, which [scope.md](scope.md) names. On another branch, it is in `${TMPDIR:-/tmp}/code-review-<repo>-<branch>`, with the branch of the head of the scope as `<branch>`, and each `/` as `-`. The file is outside the repository, so the read-only rule allows it. If the file exists, add the new report after the old report.

The first line of each report in the file is `Head: <hash>`, with the full hash of the head of the scope. For uncommitted work, add ` with uncommitted changes` after the hash.

## The owners

Give each kept finding the skill that fixes it, and its next step:

- `tdd`: a defect with a known cause, and its regression test;
- a unit of its own, as [sequence-verifiable-units](../../principles/references/sequence-verifiable-units.md) says: a refactoring of the code that the change touches;
- `debug`: a defect with an unknown cause;
- `to-feature` or `to-prd`: a gap or an error in a scenario or an FR;
- `grilling` first: a product choice that nobody made;
- `architect`: a wrong design;
- `test-audit`: a test that cannot fail;
- a draft follow-up issue: work outside the scope.

`code-review` fixes nothing. The user starts the fixes with one request. The review ends with its report, so its read-only rule and its stop rules do not apply to the fixes. Then the agent runs the skill of each owner on the branch of the scope. Each fix is a commit on that branch.

After the fixes, run `code-review` again on the diff of the fixes only, once. In that second review, the report of the first review is an intent source. Each kept finding that the fixes leave open keeps its rule of step 5. If that review gives `MERGE`, `pr` follows.
