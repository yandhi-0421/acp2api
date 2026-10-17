# Separate Before You Serialize Shared State

When two writers can change one target, first remove the shared write: give each writer its own target. Serialize the writes only when one shared target is a real invariant, and then serialize by structure.

## Why

Concurrent writes to one target cause races. A race occurs only sometimes, it is hard to reproduce, and it is expensive to debug. When two modules write one state or keep copies of it, a change to one writer cannot see the rules of the other. Thus their rules diverge. Separate targets remove both problems. Serialization keeps the shared target: it only makes the writers wait, and it adds its own failures, for example a stale lock.

## When it applies

It applies when two or more writers can change one target. A target can be a file, a branch, a key, a table row, a cache entry, or a state object. The writers can be concurrent, for example processes, threads, jobs, or parallel subagents. They can also be modules of one program that change one state in turn.

It does not apply to a target that only one writer changes, when all other code only reads it.

## Pattern

- Find each shared write target, for example:
  - a file that two writers change;
  - a branch that two writers push to;
  - an API that two modules define;
  - a state that two modules write or copy.
- By default, give each writer its own file, key, branch, or state directory. Merge the results only where a reader reads them.
- Inside a program, give each piece of state one owner. Other modules read the state, or they ask the owner to change it.

| Shared | Separate |
|---|---|
| Two jobs each write their own field into one `state.json`. | The jobs write `indexer-state.json` and `metrics-state.json`. |
| Two parallel subagents edit one index file. | Each subagent writes its own file. Then the main conversation writes the index from their files. |

- Serialize only when the writers need one canonical object, for example a counter that must not skip a value. Then serialize by structure: a lock file, sequential phases, a single writer, or compare-and-swap.
- Instructions and conventions are not concurrency control. Do not use a comment or a document line as the serializer, for example "run one at a time".
- For a race, inspect the shared state and its owners. A correct fix makes the order rule explicit in the code.

## Self-check

1. Which targets in the change have two or more writers? **Evidence:** the search command, and each target with its writers as `file:line`.
2. For each such target, what occurs if a second writer changes it at the same time? If the answer is not "nothing", do the writers need one canonical object, or do they publish independent facts? Give independent facts separate targets. **Evidence:** for each target, the answer, and the `file:line` of the separate targets, or the invariant and the `file:line` of its serializer.

## Signs that you skipped it

In code or in a diff:

- Split ownership: two or more modules write one state, or each module keeps its own copy of it.
- Two writers each change their own field in one file.
- A lock, a mutex, or a retry appears before anyone asked if the writers need one object. Treat "we need a lock" as a sign to check, not as the default answer.
- A comment or a document line is the only thing that keeps two writers apart.
- Two branches or two subagents change one file in the same round of work.

In a run:

- A failure occurs only sometimes, for example only under load or in parallel runs, and a rerun passes.
- A fix of a race adds a delay, and the order rule stays implicit.

## Relations

- [make-operations-idempotent](make-operations-idempotent.md): it holds runs one after the other, and a lock that repairs itself after a crash. If you serialize with a lock file, use that kind of lock.
