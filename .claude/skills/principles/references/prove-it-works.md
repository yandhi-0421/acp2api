# Prove It Works

Before you say that work is done, observe the real result, not a proxy. For each fact that a decision depends on, give the evidence and its proof level, or write "unproven".

## Why

If you do not check the work, its correctness is unknown. An action on a wrong inference costs more than a direct check. Indirect checks seem cheaper, but they mislead: a file timestamp, the age of an output, a self-report, or a cached screenshot. A write-up that sounds right proves nothing, because it also sounds right when it is false. The reader of your report cannot check your claims alone. A command and its output let the reader check them again.

A measured number has the same problem. A wrong run still prints a number that looks correct. Failed requests, skipped or cached work, code that never ran, a default configuration, and noise all give such numbers. For a performance number, if you cannot say what limits it, you do not know what you measured. A test: say why the result is not two times better.

## When it applies

It applies when you:

- say that a task or a fix is complete, or that a check passed;
- trust, report, or act on a measured number, for example a speed, a throughput, a latency, or an evaluation result;
- state a fact that a decision depends on, in a report, a review, a diagnosis, or a design;
- claim how a library, an external API, or another dependency behaves, in a report or in a test;
- claim that something does not exist, for example a caller, a field, or a handler.

For an output, the check shows that the output is real. For a measured number, the check also shows that the number means what you say.

It does not apply when you mark a statement as a hypothesis. Before you act on a hypothesis, or report it as a fact, get evidence for it.

## Pattern

### Observe the real result

- Run the feature. "It compiles" is not a result.
- A check that changes external state, for example a run in the real system, follows [guard-one-way-doors](guard-one-way-doors.md).
- Read the actual value, not a cached or derived copy.
- Read the diff.
- Check that a process is alive directly, not through derived state, for example a PID file.
- If a check fails, suspect the observation method before the system.
- A suite that passes, a test name, or a coverage number does not prove a behavior.
- Treat a self-report of a subagent as a lead. Also treat the output of an automated review or a scanner as a lead. Check each lead in the source.

### Script the check

- Write the check as a deterministic script that runs the same comparison again, not as a one-time look.
- If no practical test exists, use the closest executable check. Examples are a script, a command, browser automation, a snapshot comparison, and an assertion on a log line.
- When the documented targeted check is safe, run it. If none exists, run the smallest relevant check.
- Keep the script and its output for a reviewer.
- If you cannot run a relevant check, do not claim its result. Name the check and the reason.

### Proof levels

Give each fact that a decision depends on one of these proof levels:

1. A statement. Alone, it proves nothing.
2. A real `file:line`, in the project or in the source of a library.
3. A walk-through: you follow the path step by step, and you show if the claimed result occurs.
4. A run: a script or a test that calls the real code and fails if the fact is false. Often, it is one small script that imports the library that the project ships and calls the exact function.
5. A reproduction in the real system while it runs.

- Reach the highest proof level that is cheap. Say the proof level that you reached.
- If you cannot prove a fact, write "unproven" next to it. A change outside the repository, for example in a deployed configuration, is unproven until you observe it.
- Cite a real `file:line`. Never invent a caller or an API.

### Sources and absence

- For a claim about a library or an external API, cite its source at the pinned version, or a run.
- Read the source or the types of the dependency, with each local patch.
- A search that finds nothing is an answer. Report it with its command and its scope.
- A search alone does not prove an absence. Before you claim that a field, a type, or a handler does not exist, check its authoritative source. Examples are a schema, a migration, ORM metadata, a macro, an annotation, and generated API code.

### Measured numbers

A measured number is a claim. Rule out that it measured something else. For a performance number, also find what limits it.

- The limiter is the resource or the code path that limits a performance result. Examples are a CPU core, a lock, the disk, the network, and the load generator.
- Get the limiter from a profile or from the counters of a run. Map it to the source. A guess from the code is not a limiter.
- List what else the number could measure. Rule out each cause with evidence. The usual causes are:
  - errors;
  - skipped or cached work;
  - a default configuration on one side of a comparison;
  - noise;
  - a part too small to change the full result.
- For an evaluation result, ask: did each run do the task? Does the difference hold across trials and models? Does the scenario matter?
- After a fix, measure again with the same workload, on the path that the user reported.
- Keep the run count and the spread with the number. For a performance number, also keep the limiter.

## Self-check

Before you say that work is done, or report a fact, answer each question that fits.

1. Which check did you run on the real result? **Evidence:** the command, the exit status, the failures, and the limits. For a script, also its path. For each relevant check that you did not run, its name and the reason.
2. Which facts does the decision depend on, and which proof level did each fact reach? **Evidence:** each fact with its proof level and its proof: the command and the output line, or the `file:line`. Mark each fact at proof level 1 "unproven".
3. Which source or run supports each claim about a library or an external API? **Evidence:** the pinned version and the `file:line` in the source or the types, or the command and the output of a run.
4. Which authoritative source supports each claim that something does not exist? **Evidence:** the `file:line` of that source, and the search command with its count of hits.
5. What else could each measured number measure, and what limits each performance number? **Evidence:** the run count, the spread, and the evidence that rules out each other cause. For a performance number, also the limiter, with the profile or the counter that shows it, mapped to a `file:line`.

## Signs that you skipped it

The claim rests on a proxy:

- The report says "done" after a compile, a self-report, or a file timestamp, but nobody ran the feature.
- The report gives a suite that passes, a test name, or a coverage number as the proof of a behavior.
- A claim about a library gives no version, no source line, and no run.
- A claim of absence rests on one search.
- A `file:line`, a caller, or an API in the report does not exist.
- A fact that a decision depends on has no proof level, no proof, and no "unproven" mark.

The number has no explanation:

- The evidence for a number has no run count or no spread. The evidence for a performance number has no named limiter.
- The claimed time saved is larger than the total time of the changed part.
- A benchmark of one small part became faster, but the path that the user reported is still slow.

## Relations

- [test-behavior-not-implementation](test-behavior-not-implementation.md): it decides if a test can fail for a defect. A test is evidence at proof level 4 only if it can. When no practical test exists, the closest executable check here replaces the test.
- [build-the-lever](build-the-lever.md): it holds where a script stays until the work merges, and when to commit it. A check script follows the same rule.
- [protect-secrets-and-personal-data](protect-secrets-and-personal-data.md): it holds the redaction of secrets and personal data. Apply it to each output that you give as evidence.
