# From FR to scenario to test

Sections: the FRs and the scenarios, the scenarios and the test code, the path of each FR, the coverage table, fake answers.

Read the section "FR tags" of [gherkin.md](../../to-feature/references/gherkin.md). Find `<prd>` and `<dir>` with the [document map](../../domain-modeling/references/document-map.md).

In the workflow, the PRD is the PRD that the parent issue names. The parent issue has the title `<key>` of the branch `implement/<key>`. In another scope, it is the PRD that the user names. If the user names none, it is the PRD whose feature directory holds the scenarios in scope. If no PRD applies, the scope has no FR.

For the scope of the integration branch, or for a named PRD, each current FR of the PRD is in scope. In each other scope, the FRs in scope are the FRs of the scenarios in scope.

## The FRs and the scenarios

Run the four coverage commands of gherkin.md. The coverage passes as that section says. An FR without a scenario is a gap. Its owner is `to-feature`.

## The scenarios and the test code

An issue is done when a merge commit `Merge issue #<n>` on the integration branch names it, and no scenario with its slice tag has `@wip`. A merged issue with `@wip` is reopened. Check these items:

- The dry run of the runner lists each scenario of each done issue, with no undefined, `pending`, or ambiguous step.
- List each reopened issue. Its scenarios with `@wip` are out of the default run, so they prove no coverage. Its owner is `implement`.
- Without merges of issues, the dry run lists each scenario in scope without `@wip`.
- A scenario without a slice tag is in no issue. List it. Its owner is `to-tickets`.

For each scenario, find the step definitions of its `Then` steps. Judge each one as a test, as step 2 of `SKILL.md` says. A step definition of a `Then` that asserts only that no error occurred passes for a wrong implementation. So does a step definition whose expected value comes from the code under test.

## The path of each FR

The production lines that the merge of issue `<n>` changed belong to the FRs of the scenarios with the slice tag `@issue-<n>`. The first command lists the merges of the issues. The second command lists the changed lines of one merge. The third command lists the FR tags of the scenarios of issue `<n>`.

```sh
git log --first-parent --format='%h %s' <base>..HEAD | grep -E '^[0-9a-f]+ Merge issue #[0-9]+'
git diff --unified=0 "<merge>^1" "<merge>" -- <production paths>
grep -rh -B1 -E '^\s*(Scenario|Scenario Outline|Scenario Template|Example):' <dir> | grep -E "@issue-<n>([[:space:]]|\$)" | grep -oE '@FR-[0-9]+' | sort -u
```

A refactoring issue has no scenario, so its lines belong to no FR. Without merges of issues, the path of an FR is the production code that the step definitions of its scenarios call. The same is true for an issue whose merge changed no production line. Each line of such a path counts as changed. In a scope without a PRD, the mutants have no FR.

## The coverage table

The coverage table gives each FR one row, with these columns:

- the FR;
- its scenarios;
- its test code: the step definitions and the other tests, as `file:line`;
- the mutants on its path that a test killed;
- the mutants on its path that survived, and the equivalent mutants;
- each gap, with its owner.

The coverage of an FR is proven when a test of that FR kills a mutant on its path.

## Fake answers

A fake answer passes the literal values of a scenario, but it does not follow the rule of the FR. Compare the code with the rule of each FR, not with the literal values of its scenarios. A branch on a literal value of a scenario, for example `if (total === 15)`, is a lead. The mutant that returns the literal value tests it ([mutation.md](mutation.md)). A fake answer in the product code is a product defect. Its owner is `tdd`.
