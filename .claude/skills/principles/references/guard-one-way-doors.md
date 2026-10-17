# Guard One-Way Doors

Classify each action and each decision as a one-way door or a two-way door. Go through a one-way door only with the confirmation of the user and a recovery plan.

## Why

You can walk back through a two-way door, but not through a one-way door. A wrong two-way door costs one revert. Nothing undoes a wrong one-way door, so its cost has no limit. Thus the care that a step needs depends on its door, not on its size. A change that is cheap to roll back has a lower risk.

The user cannot check each technical step. Thus the confirmation before a one-way door is the last point at which the user can prevent the harm.

## When it applies

It applies before an action or a decision that can be a one-way door. An action is a one-way door when one revert cannot undo all its effects. Effects include data, a published contract, and a message that was sent.

These actions are always one-way doors in the workflow:

- a push to a remote;
- a merge into the default branch;
- a deletion that version control cannot restore, for example of uncommitted changes or untracked files of the user, a stash, or data in a database;
- an external message or publication: an issue, a comment, a pull request, an email, or a chat message;
- a run of the code of a pull request from a fork, also through a lever, because that code is untrusted.

A pull request is from a fork when `isCrossRepository` of `gh pr view` is true.

Two kinds of deletion are not one-way doors. The first kind is a file that you created in this task and that holds no work of the user. The second kind is output that a command rebuilds, for example a test database that the tests rebuild.

The definition also makes other actions one-way doors. Examples are a write to a production database or to a third-party service, and the release of a package.

For a decision, imagine a later change of mind. The decision is a one-way door when that change needs one of these:

- a migration of stored data;
- a change for external users;
- a change across much of the code that a lever cannot make in one verified unit.

Examples of one-way decisions:

- a change of the format or the meaning of stored data, because the data then needs a migration;
- a change of a published contract, because external users depend on it;
- a dependency that much of the code uses, because its replacement is a migration;
- an architectural shape, for example a monorepo or event sourcing, because much of the code follows it;
- which bounded context owns a kind of data, because a later move of the data needs a migration;
- a change of a security rule, because no revert undoes an exposure of data.

Most other decisions are two-way doors. For example, the name or the shape of one internal module is a two-way door, because a refactoring of a few files undoes it.

If you are not sure of the door of an action or a decision, treat it as a one-way door.

It does not apply to a two-way door: a two-way door needs no confirmation and no recovery plan. Examples are a local edit, a local commit, a new local branch, and a check in an isolated copy. Exception: the section "Protect the working tree of the user" applies to each edit, also to a two-way door.

## Pattern

### Before a one-way action

- Look for a two-way door that does the same job. If one exists, use it. Examples are a move to a backup location instead of a deletion, and a draft for the user instead of a post.
- Write the recovery plan: the step that undoes or limits each effect, and the backup that this step needs. Make the backup before the action, for example a backup branch or a database dump.
- Ask the user for a confirmation. Name the action, its target, its effects, and the recovery plan.
- A request of the user that names the action and its target is its confirmation. This is also true when the user gives the request with the approval of a design.
- The approval of a plan or a design does not confirm a one-way action in it, unless the words of the user name the action.
- A confirmation covers only the actions that it names, and the steps that these actions need. An example of such a step is the push of the branch for a pull request. For each later one-way door, ask again.
- A step that overwrites or deletes needs its own confirmation, also inside a confirmed action. An example is a force push.
- No step undoes a sent message. Thus, before you send a message, show its full text to the user. The text of a pull request is a message too. Wait for the confirmation. This is also true when the request named the action, unless the request also gave the text.

### Before a one-way decision

- Before each one-way decision that no approved design lists, present the options to the user. An example is a fix that changes a published contract, the meaning of stored data, or a security rule. Give the effects and the way back of each option.
- In a design that goes to the user for approval, list each one-way decision at the top. Give its effect and its way back. Then the approval confirms these decisions.
- In each report or pull request, state the door of the change.

### Protect the working tree of the user

- Before you change files, run `git status` and `git diff` again, also when you ran them before the approval. Save their output. Keep each change of the user that is not part of your task.
- In a review and in a bisection, run each check that writes files in an isolated copy. An isolated copy has its own state, for example a git worktree.
- To review uncommitted changes in an isolated copy, apply the output of `git diff HEAD --binary` from the working tree of the user. Without `--binary`, `git apply` fails as a whole on a binary change. Also copy the untracked files. Then compare `git status` in the copy with `git status` in the working tree of the user. The two must list the same files.
- For the staged changes only, apply the output of `git diff --cached --binary` with `git apply --index`. Copy no untracked file, because the staged changes do not hold it. Then compare `git diff --cached --name-status` in the copy and in the working tree of the user. The two must list the same files.
- For the run before the change in such a review, apply only the changes to test files.
- In your own work, a self-check can change a file of your task, for example a planted regression. Before such a change, save a copy of the file outside the working tree. Undo the change with an edit, not with `git checkout`. That command also removes the other uncommitted changes in the file. After the removal, run the check again. Then show an empty `cmp` of the file against the copy, and the output of `git status --short`.
- A test run that writes only ignored output needs no isolated copy. Examples of such output are a cache, a coverage report, and build output.
- A check that changes external state needs a test environment. Examples are a write to a shared database or to a third-party service, and a sent message. Without a test environment, the check is a one-way action.
- If no isolated copy exists, create a git worktree. If you cannot, skip the check. Give the reason in the report.

## Self-check

1. For each one-way action, where is the confirmation of the user, and what is the recovery step? **Evidence:** the action, the quote of the confirmation, and the recovery step or the backup path, for example a branch name or a dump file.
2. Which changes of the diff can be one-way decisions? Search the diff for these changes:
   - a migration, a file of a published contract, or security configuration;
   - a type, a field, a key, or a route that the code writes to storage, sends outside the process, or exports from a published package.

   **Evidence:** each search command, and each hit with its door.
3. For each one-way decision, which option did the user choose? **Evidence:** the quote of the user, or the line of the approved design that lists the decision.
4. Is each change of the user still in the working tree? **Evidence:** the output of `git status` before and after your work. Also the `git diff` that you saved before the work, and for each hunk of the user, the `file:line` where it stands after the work.

## Signs that you skipped it

In a run or a transcript:

- A push, a merge, a deletion, or an external message occurs with no confirmation of the user before it.
- A command such as `git push --force`, `git reset --hard`, `git clean -fd`, `git checkout -- .`, `git stash drop`, `rm -rf`, or `DROP TABLE` runs with no backup.
- A check or a bisection ran in the working tree of the user, and changes of the user are gone after it.

In a report or a pull request:

- It says "pushed", "merged", or "sent", but it quotes no confirmation and names no recovery step.
- It does not state the door of the change.
- The change makes a one-way decision that no approved design lists, and no options went to the user first. An example is a fix that changes a published contract.

## Relations

- [no-stops-after-approval](no-stops-after-approval.md): it holds the stops after an approval. A one-way door is one of them, so the approval of a design alone does not confirm a one-way action in it.
- [protect-secrets-and-personal-data](protect-secrets-and-personal-data.md): it holds the secret scan. Run the scan before each push and each publication.
