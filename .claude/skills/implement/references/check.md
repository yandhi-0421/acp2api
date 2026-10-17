# The merge checks

Sections: the checks, a failed merge check, an approved change.

Run these merge checks on the issue branch, in the order below, from the project root. Replace the placeholders as [commands.md](commands.md) says. Run each check yourself, and read only its result line and its failure names. Send a large output to a subagent, and keep only its result. All subagents use one model, so their agreement is weak evidence. Prove an important claim with code that runs.

## The checks

1. **The secrets.** Apply [protect-secrets-and-personal-data](../../principles/references/protect-secrets-and-personal-data.md). Here it changes what you scan: each commit of the issue branch, before the merge. Run the secret scan of commands.md with the `<diff command>` that it gives for a merge. Read each hit as that section says.
2. **The tree and the tip.** The working tree is clean, and the issue branch holds the tip of the integration branch. If it lacks the tip, merge the tip into it. If that merge conflicts, follow "A conflict" in commands.md. Then the merge checks run on the merged result. The first command prints nothing when the tree is clean.

   ```sh
   git status --short
   git merge-base --is-ancestor implement/<key> HEAD || git merge --no-edit implement/<key>
   ```
3. **Done when.** Show each "Done when" line of the issue with a command. If no command can show a line, show it by the diff, and mark it "unproven".

   For a slice issue, count the scenarios of the slice tag. Use the first two commands below, on the issue branch and at the tip. The count must be above 0 and equal at both places. The third command prints each scenario of the issue that still has `@wip`. Its output must be empty. The pattern ends with a space or the line end, so `@issue-12` does not match `@issue-123`.

   ```sh
   grep -rhE '@issue-<n>([[:space:]]|$)' <dir> | wc -l
   git grep -hE '@issue-<n>([[:space:]]|$)' implement/<key> -- <dir> | wc -l
   grep -rnE '@issue-<n>([[:space:]]|$)' <dir> | grep -E '@wip([[:space:]]|$)'
   ```
4. **All checks.** Apply [sequence-verifiable-units](../../principles/references/sequence-verifiable-units.md). Here it sets the pass rule of this check. Compare the failures of all checks by name, and the count of tests of each check, with `<run>/start.txt`. The issue must show no failure that `start.txt` lacks. Unless the report gives the evidence that [test-behavior-not-implementation](../../principles/references/test-behavior-not-implementation.md) requires for its deletion, a test that is now absent counts as a failure. The last commit of the issue must pass, but an earlier commit may hold a red test.
5. **The acceptance contract.** Run the check of the section "The contract check" in [freeze.md](../../bdd/references/freeze.md). If the issue is a slice issue, run it for the freeze of this issue. Run it also for the freeze of each done slice issue. Find each freeze commit as the section "Find the freeze commit" says. A slice issue without a freeze commit fails, also when a script built the issue.
6. **The hunks.** Apply [subtract-before-you-add](../../principles/references/subtract-before-you-add.md). Here it applies to the whole diff of the issue. Map each hunk to the issue. A hunk in a feature file is only the removal of `@wip` from a scenario of this issue. Map each change that the written design does not name to an agent decision or a question. The tip is an ancestor of the issue branch, so these commands show exactly the change of the issue.

   ```sh
   git diff --stat implement/<key> HEAD
   git diff implement/<key> HEAD
   ```
7. **The doors.** Apply [guard-one-way-doors](../../principles/references/guard-one-way-doors.md). Here it changes the search: run the searches of its self-check on the diff of the issue. Match each hit to a line of the written design or to a question. Also look for the signs in [wrong-design.md](wrong-design.md).

If each merge check passes, merge the issue as "The merge" in commands.md says. Read the merge back.

## A failed merge check

If the secret scan finds a secret in a commit, the issue fails at once and gets no retry. Then follow the rest of this section.

If another merge check fails and no stop reason applies, give the same issue branch to one new subagent. First, if the working tree is not clean, commit the unfinished work, as "Unfinished work" in commands.md says. Write the output of the failed check into `<run>/failed-<n>.txt`. Write a new brief that names that file. When the subagent returns, run the merge checks again.

If a merge check fails again, or a stop reason applies, the issue fails. Do not merge it. Commit its unfinished work, as "Unfinished work" in commands.md says. Switch to the integration branch. Keep the issue branch.

Name the cause and the smallest next action in the report. The cause is one of these:

- a question in the report of the subagent. Its stop reason can be 1, for example a change to a frozen contract file that a migration needs;
- a secret in a commit of the issue branch: tell the user at once (stop reason 3);
- a blocker, for example a secret that the project lacks (stop reason 3);
- a sign of a wrong design (stop reason 4);
- a changed acceptance contract (stop reason 1);
- a merge check that still fails.

If a stop reason applies, put the question in the report and in a message to the user at once. Do not wait for the answer. Continue with the issues that the failed issue does not block.

## An approved change

Right after the plan commit, run the check of the section "The contract check" in [freeze.md](../../bdd/references/freeze.md). Run it for each done slice issue, before the first hand-over. If one check fails, ask the user once. Name stop reason 1.

After the user approves a change to the acceptance contract, the freeze commits must follow it. The section "The freeze commit" of freeze.md says how. If the approved change is already in the plan commit, make the new freeze commits at once. If a subagent must make the change, put the approval in the brief of the rerun. Then `bdd` makes them. A change that `to-feature` made shows in the same way, at that first check or at a later merge check.
