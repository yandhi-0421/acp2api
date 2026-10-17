# A campaign

Sections: the steps of a campaign.

A campaign audits the whole test surface of one subsystem, for example one package. Only when the user asks for a campaign, do one. The rules of `SKILL.md` and of [audit.md](audit.md) apply to each group. This file adds the order of the work and the lessons of a full campaign.

Each step ends on its completion condition. Do not start the next step early.

## The steps

1. **The start.** At one commit, record the line counts of the tests and of the test support. Also record the result of each test file in scope. Keep the failures at the start in their own list. Such a failure is often a real product defect, not an old test. This step is complete when each test file in scope has a result.
2. **Groups.** Split the scope into groups along the modules that own the behavior, not along the names of the files. Include the cases of the subsystem in shared tests, and its end-to-end tests. This step is complete when each test file belongs to exactly one group.
3. **The audit table of each group.** Give each group to one read-only subagent. Give it the paths of audit.md and of [test-behavior-not-implementation](../../principles/references/test-behavior-not-implementation.md), its group, and the start list. It reads each test in full, and the items of "What to read" in audit.md. It returns the audit table of its group as text.

   The subagents use one model, so their agreement is weak evidence. Verify each `delete`, `merge`, and `repair` in the source yourself. This step is complete when each test declaration has a mark and an evidence line.
4. **The layer plan.** Start a second read-only pass from the tables. Look for a layer of tests that repeats another layer. An example is several suites that drive one shared function through one mock, next to a stronger suite with the real transport. Name the test that keeps each contract. Prefer the real transport with a fake network to a mock of a collaborator. Correct each error of the tables.

   This step is complete when each plan names these items:
   - the files to remove;
   - the test that keeps each contract;
   - the assertions to move;
   - the test-only seams that it unlocks.
5. **The edits.** Edit group by group, as "The shape of an edit" in audit.md says. Change each shared test support file in one commit only. Then two groups do not change it at the same time. Remove the test-only seams that each group unlocks. Update each place where the project lists its tests, its CI jobs, or a limit on the count of lines. This step is complete when each plan is applied and each test that keeps a contract passes.
6. **The preservation review.** Before you say that the campaign is complete, start independent read-only subagents. Give each one a group of deletions. It compares the deleted tests with the tests that stay. It looks for a contract that lost its only proof, and for a new assertion that cannot fail. The run without the suspects of [mutation.md](mutation.md) gives the first evidence.

   For each contract that you restore, run one mutant of its owner. Confirm that the test kills it. This step is complete when each gap is restored or rejected with evidence from the source, and each restored contract has a killed mutant.
7. **Product defects.** A failure at the start that stays in a kept test is a product defect. Report it with its owner, as the constraints of `SKILL.md` say. Record each unrelated problem as a draft follow-up issue. This step is complete when each failure at the start is in the report, as a defect with its owner or as an unrelated problem.
8. **Merges.** A campaign branch can live long. Merge the default branch into it. Do not rebase it. If the default branch changed a file that the campaign deleted, keep the deletion. Move the new contract into the test that keeps it. Then run the tests of the subsystem again on the merged head.

   This step is complete when these conditions are true:
   - Each new regression test of the default branch has a place.
   - The tests of the subsystem pass on the merged head.

The report of a campaign adds these items to the items of [report.md](report.md):

- the line counts at the start and at the end, with the production code apart;
- the groups, the removed layers, and the test that keeps each contract;
- the gaps of step 6, and their mutants;
- the product defects.
