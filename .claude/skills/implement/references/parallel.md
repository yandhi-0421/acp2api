# Parallel mode

Sections: the rule, the steps, the cleanup.

## The rule

Only when the user asks for it, use this mode. The default is one issue at a time, because parallel work adds conflicts and hides the order of the issues.

Apply [separate-before-serializing-shared-state](../../principles/references/separate-before-serializing-shared-state.md). Here it changes the setup: each subagent gets its own worktree and its own issue branch.

## The steps

1. Take every issue of the frontier. For each issue, make a worktree and its issue branch from the tip:

   ```sh
   git worktree add -b implement/<key>-issue-<n> "<run>/issue-<n>" implement/<key>
   ```

   If the issue branch exists, use this command instead:

   ```sh
   git worktree add "<run>/issue-<n>" implement/<key>-issue-<n>
   ```
2. A new worktree lacks the ignored files of the project, for example installed dependencies and `.env`. Make them with the commands of the project. If a secret is absent from the secret source of the project, ask the user. Name stop reason 3.
3. Write one brief for each issue. Its field "Branch" names the worktree. Start the subagents in the background, all at the same time.
4. When a subagent returns, do the merge checks of [check.md](check.md) in its worktree, and not the merge. Run each command from the root of the worktree, or with `git -C <worktree>`. Merge the tip into its issue branch first, so that the merge checks run on the merged result.
5. Merge one issue at a time. If the issue branch has no commit, use the empty commit of "The merge" in commands.md. Otherwise merge the issue branch without a commit, and then run all checks on the result.

   ```sh
   git switch implement/<key> && git merge --no-ff --no-commit implement/<key>-issue-<n>
   ```

   If each check passes, commit with the message of "The merge" in commands.md:

   ```sh
   git commit -m "Merge issue #<n>: <title>"
   ```

   If a check fails, undo the merge with the command below. Then follow "A failed merge check" in check.md.

   If the merge conflicts, undo it with the same command. Merge the tip into the issue branch in its worktree again, as step 4 says. If that merge is clean, do steps 4 and 5 again for that issue. Only if that merge conflicts, follow step 6.

   ```sh
   git merge --abort
   ```
6. If the merge of the tip into an issue branch conflicts, do not merge by hand. Run this command. It is the command of "A conflict" in commands.md for a worktree. Then give the issue to a new subagent.

   ```sh
   git -C <worktree> merge --abort && git -C <worktree> branch -m implement/<key>-issue-<n> implement/<key>-issue-<n>-old-$(git rev-parse --short implement/<key>-issue-<n>) && git -C <worktree> switch -c implement/<key>-issue-<n> implement/<key>
   ```
7. When the frontier changes, start subagents for the new issues.

## The cleanup

Only when a run of this key made a worktree and it holds no uncommitted work, remove it. Use no `--force`. Keep the branch of a failed issue. If its worktree holds uncommitted work, keep the worktree.

```sh
git worktree remove "<run>/issue-<n>"
```
