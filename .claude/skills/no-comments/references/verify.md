# Verify the work of the subagent

Sections: the diff of the run, the rules, a failed report, the checks, the refactorings.

## The diff of the run

Make the second snapshot of [scope.md](scope.md). Then run these commands. `<before>` and `<after>` are the hashes in `<run>/before.tree` and `<run>/after.tree`.

```sh
git -c core.quotePath=false diff --no-renames --name-only <before> <after> | grep -vxF -f "<run>/scope.txt"
git diff -U0 <before> <after> | awk '/^\+\+\+ /{next} /^\+/'
git diff <before> <after>
```

- The first command prints each changed file outside the scope. Only a file outside the scope that a check writes can be there.
- The second command prints each added line. An added line must be a removed line of the same hunk without its comment. Any other added line is an edit of code.
- In the full diff, each removed line must hold only a comment, or a comment that the added line no longer has. A removed blank line next to a removed comment is no edit of code.

## The rules

The main conversation can be the author of the code, and the author finds its own comments useful. Thus a restore needs an exact exception of [brief.md](brief.md) and its proof. Apply these rules:

- **A deletion that an exception covers exactly, with its proof.** Restore the comment.
- **An ambiguous deletion.** Do not restore it.
- **A keep without an exception or without proof, or with a proof that the source refutes.** Delete the comment yourself. If the keep stays ambiguous after your check, delete it too.
- **A reshape flag on code outside the scope, a flag with a false reason, or a flag on intentional code that the trace shows correct.** Drop the flag, or correct it. Do not restore the comment of a correct flag. If the trace shows a real problem outside the scope, write a draft follow-up issue.
- **A suppression in scope that the report does not name.** Judge it yourself with "Suppressions" in brief.md.

Check each piece of evidence in the source. Only when the evidence of a claim is thin, do the trace of brief.md yourself. Thin evidence has no `file:line` and no commit.

To restore a comment, add its line back with an edit. If the file holds no other accepted deletion, use this command, which changes only the working tree:

```sh
git restore --source=<before> --worktree -- <path>
```

## A failed report

A report fails in these cases:

- The subagent edited code, edited a file outside the scope, or created a file.
- The subagent made a commit: `git rev-parse HEAD` does not print the hash in `<run>/head.txt`.
- The subagent did not read a file in scope.
- In a question, the subagent edited a file.
- The report lacks a part of "Your report" in brief.md.

A file outside the scope that a check writes does not fail a report, for example a cache or a test result.

If the subagent made a commit, run `git reset --soft "$(cat "<run>/head.txt")" && cp "<run>/user.index" "$(git rev-parse --git-path index)"`. This restores the original head and the original index without an edit to the working tree. Report the recovery and the hash of that commit.

Then restore each file in the list of that subagent, with the command above. Also restore each changed file outside the scope that `<before>` holds. Delete each file that the subagent created, because it holds no work of the user. Start one new subagent with the same inputs, and one line that names the failure.

## The checks

This section is for step 4: a failure that the start list lacks.

- **A formatter check.** A deleted comment can leave two blank lines or a shorter call, which the formatter rejects. First restore each deleted comment that the formatter reads, for example `# fmt: off`. Then run the formatter on each file with a new formatter failure, because a formatter keeps the behavior. If the file holds uncommitted work of the user, restore the deleted comment instead. Report each restored comment, with the failure as its reason.
- **Another check.** A deleted comment can be a directive that a tool reads. Find the deleted comment that causes the failure. Restore it. Judge a restored suppression with "Suppressions" in brief.md. Its flag counts as accepted.

## The refactorings

Only with one of these refactorings, do the change of an accepted reshape flag yourself:

- Rename Variable, or Change Function Declaration for a new name or for a parameter that no caller uses;
- Extract Variable, or Extract Function, with a name that states the behavior;
- Replace Magic Literal;
- Remove Dead Code.

Each of these conditions must be true:

- The run commits, as scope.md says.
- The refactoring keeps the behavior.
- It changes only files in scope.
- It changes no file of the acceptance contract.
- It changes or removes no interface that an external user calls, as [migrate-callers-then-delete-legacy-apis](../../principles/references/migrate-callers-then-delete-legacy-apis.md) defines the term.
- All checks pass after it, with no failure that the start list lacks.

If a check fails after a refactoring, undo the refactoring with an edit. The flag then stays open, with the failure as its reason. Do not fix the code around it.

Give each open flag its owner, as the owners of `code-review` are:

- a refactoring that step 5 did not do, or a new type: a unit of its own, as [sequence-verifiable-units](../../principles/references/sequence-verifiable-units.md) says;
- a workaround or a defect: `tdd` for a known cause, and `debug` for an unknown cause;
- a new design: `architect`;
- a constraint to enforce: `tdd` for a test, and a unit of its own for a type, a runtime check, or a lint rule.
