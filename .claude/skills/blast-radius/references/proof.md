# Prove a safety fact

Sections: the run directory, the isolated copy, a pull request from a fork, the dependencies, the negative control, a run that fails, a fact that no run can prove.

If the report of the author names a lever for the same fact, run that lever again first. Run it where step 5 runs the script.

## The run directory

`<run>` is the directory `${TMPDIR:-/tmp}/blast-radius-<repo>-<head>`. `<repo>` is the name of the root directory of the repository, and `<head>` is the short head commit. The directory is outside the repository, so the run changes no file of the project. Keep the script there until the merge, because a later reviewer runs it again. If the script is short, paste it into the report, because the system can clear its temporary directory.

## The isolated copy

Make the copy at the head, with an absolute path:

```sh
git worktree add --detach "<run>/copy" "<head>"
```

- Before the run, record `git status --short` of the working tree of the user.
- For uncommitted work, apply the changes of the scope to the copy, as [guard-one-way-doors](../../principles/references/guard-one-way-doors.md) says. Without them, the copy holds the old code, and the script proves the fact on the wrong code. Of the untracked files that git does not ignore, copy only those that the scope holds. Then an import that fails because the scope lacks a file of the repository is a confirmed risk.
- For a planned change, apply the planned change in the copy. Make the proof there.
- After the run, remove each copy that you made, for example with `git worktree remove --force "<run>/copy"`. A copy holds only files of this task, so its removal is not a one-way door.
- Use an absolute path. `git -C <repo> worktree add <relative path>` makes the copy inside the repository.

## A pull request from a fork

Apply [guard-one-way-doors](../../principles/references/guard-one-way-doors.md). Here it makes each run of the code of the pull request or of a lever wait for the yes of the user. Under `code-review`, put the question into its report.

## The dependencies

A new copy has no installed dependencies, for example `node_modules` or a virtual environment. Use the first choice that works:

1. If the working tree of the user holds only the change, and the run writes only ignored output, use it. Give the negative control a planted input. Do not plant a fault there.
2. Install the dependencies from the lock file into the copy, for example into a new virtual environment in the copy. An install runs the install scripts of each package that it installs, also of a package that the change updates. Thus install with these scripts off, for example with `npm ci --ignore-scripts` or `pip install --only-binary :all:`. If the run needs them, use choice 3. If the change adds or updates a git dependency, use choice 3, because npm still runs its `prepare` script. Look for it also in the lock file.
3. Stop at proof level 3, and say why.

## The negative control

Make the fact false. Then run the script again. Do not change its check, for example its expected value. A script that never calls the real code also fails when you reverse its expected value, so such a failure proves nothing.

- A planted input: give the real code an input for which the fact is false.
- A planted fault: change the code so that the fact is false. Only in the copy, plant a fault.

Count only a failure at the check of the fact, not at an import. Unless the fact states which inputs the code accepts, do not count a rejection of the input. Paste the failure line. If no such case exists, say why. Before the run of the next fact, undo the planted fault.

## A run that fails

If the script fails at its check on the change, run it on the code before the change. Use a copy at `<run>/base`:

- If it passes there, the fact is false. Report it as a confirmed risk.
- If it fails there too, the script is wrong. Fix the script, or mark the fact unproven.
- If the code does not exist at the base, say so.

## A fact that no run can prove

Some facts need a system that you cannot reach, for example the clients of a public API. Give the proof level that the fact reached, and the reason why no run can prove it. Give the options and their effects, so that the user can decide. A check in a real system follows guard-one-way-doors.
