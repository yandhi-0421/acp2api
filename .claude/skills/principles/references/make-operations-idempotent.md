# Make Operations Idempotent

Make each operation that changes state converge to one correct end state from any start point. Each rerun, and each run after a crash at any step, must give the end state of one clean run.

## Why

Commands, lifecycle steps, and process loops run among crashes, restarts, and retries. These events are normal, and you cannot choose the step at which a crash occurs. If the state that a partial run leaves changes the result of the next run, each restart needs a debug session. People also run commands again on purpose, for example a reviewer who runs a lever again.

## When it applies

It applies when you write or change an operation that changes state and that can run again or stop part way. Examples are a command, a script, a migration, a start step, a scheduled job, a handler of external events, and a loop over items.

It does not apply to an operation that changes no state, for example a query or a check that only reads.

## Pattern

- Make startup convergent. Do not assume a clean start. At startup:
  - scan the state that exists;
  - clean stale artifacts;
  - adopt live sessions.
- Find earlier results by content equivalence, or by a key that the input decides. Clean up by content equivalence too. Do not use the creation order or a counter. Then a rerun finds the earlier result and makes no second copy.
- A repeat of an operation that succeeded is not bad input. Return the result of the first run, not an error.
- Where you can, make each step all or nothing. Examples are a transaction, and a write to a temporary file with a rename.
- Make a lock repair itself. Write the process ID into the lock file. Remove a lock whose process no longer runs.
- Let failed work start again cleanly. Make fresh input for each cycle. Do not reuse the partial output of a failed cycle.
- If the result depends on the state that an earlier run left, add a reconciliation step. It compares the actual state with the correct end state, and it changes only the difference.

## Self-check

For questions 1 and 2, run the operation in an isolated copy or in a test environment, as [guard-one-way-doors](guard-one-way-doors.md) says.

1. What occurs after two runs in a row? Run the operation twice on one input. Compare the two end states. **Evidence:** the command, and the diff of the end states, which must be empty.
2. What occurs after a crash at each step that changes state? Answer for each step. Then stop the operation after the step that leaves the most partial state. Run it again. Compare the end state with the end state of one clean run. **Evidence:** the step, how you stopped the operation, the command, and the diff, which must be empty.

If an answer is "it depends on the state that the earlier run left", add the reconciliation step. Then answer again.

## Signs that you skipped it

In a run:

- A second run fails, for example with "already exists", or it makes a second copy of a row, a file, or a message.
- After a crash, the next run fails until somebody deletes a lock file, a temporary file, or a state file by hand.

In code or in documents:

- The code takes the newest or the last item, and it assumes that the current run made it.
- A lock file holds no process ID, and no code removes a stale lock.
- A retry repeats a step that is not safe to repeat, for example an insert or a send.
- A failed cycle uses its own partial output as the next input.
- A document says "run this only once" or "start from a clean state".

## Relations

- [separate-before-serializing-shared-state](separate-before-serializing-shared-state.md): if two runs can occur at the same time, apply that principle too. This principle holds runs one after the other, and runs after a crash.
