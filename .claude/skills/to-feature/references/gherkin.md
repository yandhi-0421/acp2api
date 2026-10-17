# Write the Gherkin

Sections:

- keep the files executable;
- structure;
- clear and stable scenarios;
- FR tags;
- slice tags and run-control tags;
- revise scenarios;
- review rules;
- validate the syntax.

Keep the product behavior before the project style. A convention of the project must not hide or change the result that the PRD states.

## Keep the files executable

Keep questions, assumptions, coverage notes, review findings, delivery tasks, and internal implementation work out of the `.feature` file. Keep unsettled scenarios and vague placeholders out of settled Gherkin. An unsettled scenario is a scenario that an open decision blocks.

Put each settled scenario in the file, also when its issue comes later. When an open decision blocks a scenario, write the other scenarios. List the blocker. When an open decision blocks every result, return the blockers.

## Use the Gherkin structure with purpose

Put one `Feature` in each file. Name it for a stable business capability. The description can state the value, the scope, and the source. It must not repeat the requirement.

Use `Rule` only for a real business rule that has several examples. A single clear rule needs no `Rule` block. Put the conditions of a scenario in its steps, or in a short `Background` that applies.

Name each `Scenario` for the behavior that it shows. Do not use a ticket number or a label such as "happy path". The FR ID is in the FR tag, not in the name.

Use the step roles as follows:

- `Given` states the prior state.
- `When` states the action or the event.
- `Then` states an observable result.
- `And` and `But` continue the role of the step before.

Use only the roles that the scenario needs. Do not write an empty step to fill a template.

Use `Background` only for a short executable prior state that every scenario in its scope shares. Keep important rules, large fixtures, and scenario-specific state in the scenario that needs them.

Only when the steps are the same and only the data changes, use a `Scenario Outline`. Match each placeholder to an `Examples` header. Give every header a value in each row. Remove each unused column.

When one step takes structured domain values, use a data table. Use a doc string for meaningful text of several lines. Keep fixtures of the implementation and bulk test data outside the feature file.

Use the English keywords in the feature files of the workflow, also when the repository uses another dialect. Declare the dialect in each file with the header `# language: en`. Follow the repository in its choice of `Scenario` or `Example`. Reuse the tags and the source identifiers that have a meaning.

Write each title, description, and step in English STE (see `ste.md`). The Gherkin keywords keep their form.

## Make scenarios clear and stable

Each scenario shows one rule result through one action or event. When the order or the repeat is the rule, use a short sequence of events. Separate independent actions and unrelated results.

State all prior state that can change the result: time, identity, ownership, records, and external conditions. Make the scenarios independent of the run order and of random data. If the behavior is a dependency on one of them, keep that dependency.

Use concrete and consistent values. A reader must be able to derive the result. Use only the words, the precision, and the rule values that the PRD states.

Use one term for each domain concept, from `GLOSSARY.md`. For that concept, use no word from the Avoid column of its entry. If a step needs a term that the glossary lacks, load `domain-modeling`. Keep one grammatical viewpoint in a feature. When the meaning is the same, reuse the step text. A long scenario is a reason to check for combined behavior or procedural detail.

### Write the domain intent, not the interface

Write domain intent in each step. A step names what the actor does and gets, in domain terms. Do not write navigation, selectors, private calls, or database mechanics.

Only when the FR itself is about the interface, name a control, for example a button or a field. Examples are an FR about a public API, an accessibility interaction, or a regulation.

Reason: a step about the interface breaks when the interface changes, but the behavior has not changed. It also hides the rule that the scenario must show. Then a reader cannot tell which result the FR demands.

### Write one scenario for each equivalence partition

Write one scenario for each equivalence partition and each boundary value of an FR. A boundary value is the value on each side of a point where the result changes. An example is 499 and 500 for "500 EUR or more". A second scenario in the same equivalence partition is a duplicate example, unless it is a boundary value. Each row of a `Scenario Outline` is a different equivalence partition or boundary value.

Reason: a duplicate example adds review work and run time, and it gives no new evidence. Many near copies also hide the few scenarios that matter.

### Name a result that a wrong implementation changes

Each `Then` names an observable result that a wrong implementation changes. The result can be a message, an available action, a public response, a business event, a durable outcome, or other observable evidence. The value comes from the FR.

A `Then` that says only that no error occurs, that the action works, or that a value appears, passes for a wrong implementation. Such a scenario gives false confidence. If the FR states no exact value, name the property that the FR states. An example is a generated order number.

Check each scenario with one question: which close wrong implementation fails it? If you cannot name one, change the `Then`, or delete the scenario.

The principle [test-behavior-not-implementation](../../principles/references/test-behavior-not-implementation.md) gives the reason and the signs of a test that cannot fail. Here it changes one thing: the expected value comes from the FR, not from the code. This skill runs no test. Thus the wrong implementation that you name is the evidence for question 2 of the self-check of that principle.

## FR tags

Each scenario carries the FR tag of each functional requirement (FR) of the PRD that it covers. The skills `prd-review`, `to-tickets`, `bdd`, and `test-audit` read these tags. They check the coverage from FR to scenario to test.

- Write the tag as `@` and the FR ID, for example `@FR-001`. Copy the ID from the PRD. Do not invent an ID.
- Put all FR tags of a scenario on the one line above the `Scenario` or the `Scenario Outline`. Do not put them on a `Feature` or a `Rule`. A tag on a `Feature` gives that FR to every scenario in the file, so the tag does not show which scenario covers the FR.
- Give each scenario one FR tag. Only when one result belongs to two FRs, add a second tag.
- Give all rows of a `Scenario Outline` the same FR. If the rows belong to different FRs, split the outline.
- Do not tag a scenario with an SC. Do not add a slice tag or a run-control tag. Keep each one that exists. The section "Slice tags and run-control tags" names the skills that add them. The section "Revise scenarios" gives the one exception.
- Keep the other tags of the repository. Write them after the FR tag, on the same line.
- Give each FR of the PRD at least one tagged scenario. If an FR has none, state the reason in the report.
- Use only the FR IDs that the PRD contains now. When the PRD retires an FR, remove its scenarios or move them to a current FR.

Run these coverage commands, where `<prd>` is the PRD file and `<dir>` is the `<feature>/` directory of this PRD. The PRD lists a retired FR on a line such as `FR-004 — Retired: replaced by FR-007`. The first command reads only the section whose heading names the requirements. It reads only the FR ID at the start of each list item, and it skips each line with `— Retired:`.

```sh
awk '/^## /{s=/^## .*[Rr]equirements/} s' <prd> | grep -v -- '— Retired:' | grep -oE '^[-*] \**FR-[0-9]+' | grep -oE 'FR-[0-9]+' | sort -u
grep -rh -B1 -E '^\s*(Scenario|Scenario Outline|Scenario Template|Example):' <dir> | grep -E '^\s*@' | grep -oE '@FR-[0-9]+' | tr -d '@' | sort -u
grep -rhE '^\s*(Scenario|Scenario Outline|Scenario Template|Example):' <dir> | wc -l
grep -rh -B1 -E '^\s*(Scenario|Scenario Outline|Scenario Template|Example):' <dir> | grep -cE '^\s*@FR-[0-9]+'
```

The coverage passes when:

- Command 1 lists the current FR IDs, and command 2 lists the FR IDs on the tag lines above the scenarios. Each ID of command 1 is also in command 2, or the report explains it. Command 2 has no ID that command 1 lacks.
- Command 3 counts the scenarios. The count is above 0, or the report gives the blockers. Command 4 counts the scenarios that have an FR tag on the line above. The two numbers are equal, or the report explains the difference.

## Slice tags and run-control tags

The skill `to-tickets` adds a slice tag and a run-control tag to each scenario that it puts into an issue. It adds a run-control tag alone to each scenario that it does not slice. The skills `implement` and `bdd` read them. The skill `to-feature` adds neither. The section "Revise scenarios" gives the one exception.

- The slice tag is `@issue-` and the number of the GitHub issue that holds the scenario, for example `@issue-12`. The tag names the scenarios of one issue, so the scope of `bdd` is the tag of its issue.
- The run-control tag is `@wip`. It keeps a scenario out of the default test run until the scenario is in an issue and that issue is done. The scenarios of later issues, and the scenarios that are not sliced, have no step definitions, so without the tag the default run stays red. The name follows the convention of Cucumber.
- Give a scenario at most one slice tag. A scenario without a slice tag is not in an issue.
- Write both tags on the line of the FR tags. Put them after the FR tags and after the other tags of the repository, for example `@FR-001 @issue-12 @wip`. A `@wip` that is already on the line stays where it is, so the slice tag can follow it. The line still starts with an FR tag, so the coverage commands of "FR tags" give the same result.
- Do not put either tag on a `Feature` or a `Rule`. A tag there gives it to every scenario below it, so it does not show which scenario is in the issue.
- `to-tickets` adds both tags after the issue exists with its sub-issue link, its label, and its blocking edges. It changes nothing else in the file.
- `bdd` removes `@wip` from the scenarios of its issue when they pass. The first `bdd` run in a project also writes the exclusion rule, before its freeze commit. [freeze.md](../../bdd/references/freeze.md) gives the place of the rule. The rule makes the default run exclude `@wip`.
- `to-tickets` removes a slice tag only to undo its own run, or to repair a second slice tag that its run added. No other skill removes a slice tag. The tag stays on the scenario after the issue closes.

## Revise scenarios

Change only the scope that the PRD change, the review, or the test run names. Keep the scenarios that work. Run the checks of `SKILL.md` on the changed scope.

Remove an in-scope scenario only for one of these reasons:

- its behavior is a duplicate;
- the PRD rejects it;
- the fail check shows that it has no observable result;
- the PRD retires its FR;
- a new scenario replaces it, as the rule for a closed issue below says.

If a rule of the project or of the user requires an approval for a deletion, get the approval first.

A scenario with a slice tag belongs to an issue. A `Background` belongs to each issue of its file. Read the state of the issue with `gh issue view <n> --json state --jq .state`. The number in the slice tag is `<n>`. If the command fails, ask the user.

Before you change a scenario or a `Background`, find the freeze commit of each issue that it belongs to. Follow "Find the freeze commit" in [freeze.md](../../bdd/references/freeze.md), in the form with `--all`. If a commit exists, or the tag line lacks `@wip`, ask the user first. That is stop reason 1 of [no-stops-after-approval](../../principles/references/no-stops-after-approval.md). For a `CLOSED` issue, the question about a scenario comes at the removal of the old scenario. The question about a `Background` comes at once.

Revise a scenario as the state of its issue says:

- If the state is `CLOSED`, write the revised behavior as a new scenario. Give it its FR tag, and no slice tag. Only with the approval of the user, remove the old scenario, which the new scenario replaces. A rerun of `to-tickets` then slices the new scenario.
- If the state is `OPEN`, keep the slice tag. If the tag line lacks `@wip`, add `@wip` again at the end of the line. Then `bdd` of that issue automates the scenario again and freezes it again. This is the only case in which `to-feature` adds a tag.

Leave the scenarios outside the scope unchanged. Repair the other defects in scope, or report the open decision.

## Review rules

The author uses these rules for the check of the scenarios. The skill `prd-review` uses them for its review.

Report a style issue only in one of these cases:

- it changes the meaning;
- it breaks a project convention;
- it makes the behavior hard to maintain.

Review the behavior before the syntax. Valid syntax cannot fix an invented result, an absent rule, a procedural script, or a duplicate example.

## Validate the syntax

If the project has a read-only parser, compile check, or dry run, use it. Only when the user authorizes edits, run a formatter. Before `bdd` runs, each step is undefined. Do not report that as an error.

If no parser is available, check the file by hand:

- Each file has one `Feature`. Structure keywords have colons. Step keywords have no colons.
- Each `Scenario Outline` has an `Examples` table that follows the outline rule above.
- Each scope has at most one `Background`, and it comes before the scenarios.
- Each tag stands before a valid structure and has no spaces.
- Each data table and doc string is complete, and the indentation is consistent.
- No unresolved marker stays in the file.

Report which checks passed. If only static checks ran, state that parser validation is unverified.
