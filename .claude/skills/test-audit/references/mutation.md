# Mutation testing

Sections: the tool, the kinds of mutants, the order and the time, the copy, the script, the results, a run without the suspects, equivalent mutants.

A mutant changes one line of production code. If a test of the behavior fails, the test kills the mutant. A test that kills a mutant can fail for that defect. A test that kills no mutant can protect nothing.

## The tool

If the project already has a mutation tool, run it on the production files in scope only. Look for it in the manifests and in the configuration files, for example `stryker`, `mutmut`, `pitest`, or `cargo-mutants`. Its result table replaces the table of the script.

If the project has no such tool, write the mutants by hand. Run them with the script of this skill. Apply [choose-boring-technology](../../principles/references/choose-boring-technology.md). Here it forbids a new mutation tool without the approval of the user, also a tool that you run once.

## The kinds of mutants

Write mutants of these kinds:

- negate a condition: `if (a > b)` becomes `if (!(a > b))`;
- change a boundary value: `>` becomes `>=`;
- change an operator: `+` becomes `-`, or `&&` becomes `||`;
- return a constant of the return type: the empty value, or a fixed value;
- remove a call that has an effect, for example a write or a sent event;
- replace a computed result with the literal value that a scenario expects.

If the last kind survives, the tests check only one example. Then a fake answer can pass them. Do not mutate a test file, a comment, or a log line that no observer reads.

## The order and the time

Write the mutants in this order:

1. the lines that each `delete` or `merge` suspect claims to protect;
2. for each FR, each changed condition and each changed return value on its path ([coverage.md](coverage.md) gives the path);
3. the other changed production lines.

Estimate the time: the time of the run without a mutant, multiplied by the count of mutants. If the estimate is above 30 minutes, give each mutant a `cmd`. It runs only the tests of its FR and its module. If the estimate is still above 30 minutes, keep the order above. List the other lines as `not mutated` in the report.

## The copy

Run each mutant only in a copy: a worktree with a detached head at the head of the scope. Make the copy with the first command. After the last run that needs it, remove it with the second command. A copy holds only files of this task, so its removal is not a one-way door.

```sh
git worktree add --detach "<run>/copy" HEAD
git worktree remove --force "<run>/copy"
```

A new copy lacks the ignored files, for example the installed dependencies and the `.env` file. Make the dependencies with the commands of the project. A test that writes to a database or a service needs a test environment, as [guard-one-way-doors](../../principles/references/guard-one-way-doors.md) says. Thus read the `.env` file before you copy it. For a scope of uncommitted changes, apply the changes in the copy, as that principle says for an isolated copy.

Put one control mutant first for each package in scope and for each command. It changes a return value that a test of the command surely asserts. If the control mutant survives, the command does not test the code of the copy. An example is an installed package that points to the working tree of the user. Correct the command. Use no result of that run.

## The script

Write the mutants into `<run>/mutants.json`, as a JSON array. Write each line number as it is at the head of the copy. Find it with `grep -n` in the copy, because a later merge can move the line. Each object has these fields:

- `id`: a short name without `/` that does not start with `no-mutant-`, for example `FR-001-boundary`;
- `path`: the path of the production file, from the root of the repository;
- `line`: the number of the line, from 1;
- `old`: text that occurs exactly once on that line;
- `new`: the text that replaces it;
- `cmd`: optional, a test command for this mutant only.

```json
[{"id": "FR-001-boundary", "path": "src/price.js", "line": 12, "old": "qty > 10", "new": "qty >= 10"}]
```

Run the script. `<skill dir>` is the directory of this skill. If the estimate is above the time limit of one shell command, run the script in the background.

```sh
python3 "<skill dir>/scripts/mutate.py" --copy "<run>/copy" --mutants "<run>/mutants.json" --cmd '<test command>' --out "<run>/logs"
```

The script refuses a copy that is not a linked worktree with a detached head. First, it runs `--cmd` and each `cmd` once without a mutant. Each of these runs must pass. Then it applies each mutant, runs its command, and restores the file. The default time limit of one run is three times the time of its run without a mutant, and at least 60 seconds. `--timeout <seconds>` sets another limit for each run, also for the runs without a mutant.

The script prints one row for each mutant, with one of these results: `killed`, `survived`, `timeout`, or `error`. The output of each run is in `<run>/logs/<id>.log`. Do these actions for each exit status:

- 0: the run is complete.
- 2: the input is bad, or the copy is not a linked worktree with a detached head. Correct the input or the copy.
- 3: a run without a mutant failed. A dependency can be absent in the copy, or a test of the command fails. Repair the copy, or choose a command whose tests pass. Report a test that fails.
- 4: a run changed a file in the copy, or a process of a run that timed out still acts on the copy. Read the message and `git status` in the copy. For a process, stop it. For a changed file, give the mutants a command that does not write a file in the copy. Then make a new copy, as "The copy" says. Run only the mutants after the mutant of the message, whose row stays.
- 5: a signal stopped the run. Make a new copy before the next run.

The script can end without an exit status of this list, for example at a time limit of the shell. Then stop each process of the run that still runs. Remove the copy. Make a new copy before the next run.

## The results

- Read the log of each `killed` mutant. A mutant is killed only when its log shows a test that fails at its assertion. A failure to compile, to import, or to load is an `error`. So is a fixture that fails.
- A log can show a failure of the environment, for example a port in use. It is not an `error` of the mutant. Stop the process that holds the port or the file. Make a new copy. Then run again each mutant whose log shows that failure.
- For a mutant with the result `error`, correct its `line`, `old`, or `new`. Then run it again. Example: for a removed call, keep in use each variable that only the call used, as `_ = v` in Go.
- A `timeout` counts as killed. Give it apart in the report.
- To find the test that kills a mutant, read its log. Or give the mutant a `cmd` that runs one test.
- A mutant that a test of its FR kills proves the coverage of that FR on that line.
- A mutant that survives is a gap or an equivalent mutant.
- For a gap on the path of an FR, or on a line that a `keep` test protects, step 4 of `SKILL.md` adds one test. Commit the test.
- Then remove `<run>/copy`. Make a new copy at `HEAD`. Run the mutant again. The test must kill the mutant and pass without it. This run is the evidence for question 2 of the self-check of test-behavior-not-implementation.
- A gap that only a scenario can close goes to `to-feature`. An example is a `Then` step whose text checks too little.
- A held `repair` can kill a mutant that survives. Make the repair in a copy at `<run>/repair`. Run the mutant there, with `--out "<run>/logs-repair"`. If the repair kills it, add no test yet. Only after the user rejects the repair, add the test. If nobody answers, report the mutant as a gap, with the question.

## A run without the suspects

Each `delete` or `merge` of a test outside the acceptance contract needs this run. It shows that the tests that stay still kill each mutant that a suspect kills.

1. For each suspect, write the mutants of the lines that it claims to protect into `<run>/suspects.json`. Give them a `cmd` that runs only the suspect. Give `--cmd` the command of one suspect. Run them in `<run>/copy`, with `--out "<run>/logs-suspects"`. Note the mutants that each suspect kills.
2. Make a second copy at `<run>/without`, as "The copy" says. In that copy, delete all `delete` and `merge` suspects together, but do not commit. Two suspects can protect one contract, so delete them together. For a `merge`, also add the assertion to the test that takes it.
3. Write the mutants that a suspect killed into `<run>/without.json`, without the `cmd` of step 1. Run them in `<run>/without`, with the tests of their FR and their module, and `--out "<run>/logs-without"`.
4. Each mutant that a suspect killed must be killed again. If one survives, a suspect protects something. Mark that suspect `keep`, `repair`, or `merge`.

A suspect that kills no mutant, and that matches a sign of test-behavior-not-implementation, gets `delete`. The exception is a suspect whose contract the keep list of that principle names.

## Equivalent mutants

An equivalent mutant changes the code, but no observer can tell it from the code before the mutant. Only with evidence, mark a mutant that survives as equivalent. Name the observer, as test-behavior-not-implementation defines it in "What behavior is". Say why that observer cannot tell the difference. Example: a changed boundary value in a loop guard that no input can reach.

Without that evidence, the mutant is a gap. The report lists each equivalent mutant with its evidence, so that `code-review` can check it.
