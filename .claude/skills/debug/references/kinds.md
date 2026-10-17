# Kinds of bugs

This file is the depth of steps 1 to 3 of `debug` for some kinds of bugs. Read only the sections that fit.

Sections: no local reproduction, unstable and concurrent bugs, bisection, state and lifecycle, performance, leaks, integration and data, an incident.

A write to a real system that this file names, and that the user confirms, is an exception to "Local work".

## No local reproduction

- If you cannot reproduce the bug locally, replay a captured artifact as the reproduction check. Examples are a redacted request, an event, a trace, a log, and a dump.
- To capture such an artifact, you can add temporary instrumentation. On a real system, the instrumentation is a write, so it is stop reason 2.
- Before the check exists, a guess can only choose where to capture evidence. It is not a ranked cause, and it changes no product behavior.
- If no check results, stop with `Blocked`, as the stop rules of the skill say.

## Unstable and concurrent bugs

- Before you reason from one run, measure the failure rate. When they matter, record the seed, the worker count, the times and the order of events, and the machine load.
- Raise the rate until the bug is debuggable. A bug that fails in half of the runs is debuggable, but a bug that fails in 1 run of 100 is not. Use these means:
  - repeat the trigger 100 times;
  - run it in parallel;
  - add load;
  - narrow the time window.
- A delay can expose an order defect.
- Inspect the shared state, the ownership, cancellation, timeouts, locks, the message order, and the lifecycle transitions.
- If two writers can change one target, apply [separate-before-serializing-shared-state](../../principles/references/separate-before-serializing-shared-state.md). Here it gives the shape of the fix: no shared target, or an explicit order rule in the code.
- In step 3 and in the bisect wrapper, repeat the check. Choose the count from the rate of step 2, so that a bad state almost never passes. The repeated check passes only when each run passes. At a rate of one half, 10 runs miss the bug about once in 1,000 states.
- After the fix, compare the failure rate with the rate of step 2.

## Bisection

Apply [guard-one-way-doors](../../principles/references/guard-one-way-doors.md). Here it puts the bisection in an isolated copy, for example a git worktree. Give the copy the dependencies that the build needs. If the dependency files change between `<good>` and `<bad>`, install the dependencies in the wrapper at each revision. Use an install that keeps the lock file. If you cannot make an isolated copy, use the read-only history of [techniques.md](techniques.md).

The wrapper runs only the reproduction check, never the whole suite. For an unstable bug, it repeats the check, as "Unstable and concurrent bugs" says. Inside an issue, a commit can hold a red test before the commit of its fix. At such a commit, the suite fails for a reason that is not this bug.

```sh
#!/bin/sh
# Exit 125: this revision cannot run the check.
<precondition> || exit 125
out=$(<reproduction check> 2>&1) && exit 0
printf '%s\n' "$out" | grep -F -q -- '<exact symptom>' && exit 1
exit 125
```

The precondition fails when the test of the check does not exist at that revision. It also fails when the scenario of the check still has `@wip` there, because then the scenario is red by design. Exit 0 means good, 1 means the bug, 125 means skip, and 128 or more stops the bisection. A failure for another reason, for example a broken build or install, is a skip, not the bug.

In the wrapper, turn off or clear the caches of the build and of the runtime. A fast checkout can leave a stale cache, and then the result is wrong.

Before the bisection, run the wrapper at `<good>` and at `<bad>`. Only when it exits 0 at `<good>` and 1 at `<bad>`, start the bisection. If it does not, first examine the wrapper and the copy. Then you can choose another `<good>`.

```bash
git bisect start --first-parent <bad> <good>
git bisect run <wrapper>
git bisect reset
```

- On a branch with merge commits, for example the `Merge issue #<n>` commits of `implement`, use `--first-parent`. Then the bisection does not test the commits inside a merged branch, where a red test can be.
- On a branch without merge commits, leave out `--first-parent`.
- If the first bad commit is a merge commit, run the wrapper at its second parent. If the wrapper exits 0 there, the merge itself is the first bad commit.
- If the wrapper exits 1 at the second parent, bisect inside the merged branch, without `--first-parent`. In the command below, `merge` holds the hash of the merge commit.
- If git names a range of commits that it could not test, report that range as the result.
- At the end, and also after an interruption, run `git bisect reset`.

```bash
git bisect start "$merge^2" "$merge^1"
```

## State and lifecycle

Reproduce the event sequence, not only the final input. Record the transitions that lead to the wrong state. Inspect the start, the cleanup, transactions, callbacks, cache invalidation, retries, and changes of ownership. Reset the state between runs, so that you can tell persistent corruption from a defect of the input.

## Performance

- Measure first. Fix second. Logs are usually the wrong tool here.
- Before you change code, define the workload, the measured value, and the acceptable range.
- Keep the environment stable. When the runtime needs a warm-up, include one. Compare a set of measurements, not one result of a clock.
- Apply [prove-it-works](../../principles/references/prove-it-works.md). Here it adds the limiter, the run count, and the spread to each number.
- Make the benchmark a check: outside the range, it exits 1 and prints the same line each time. Then you can bisect the history with it.
- Use the tool that fits the resource:
  - a profiler for CPU time and call paths;
  - a query plan for database work;
  - heap tools for memory;
  - I/O and network counters for external waits;
  - lock traces for contention.

## Leaks

Repeat a constant workload. Track the retained memory, file descriptors, sockets, connections, tasks, or handles. Find where the code gets the resource, who owns it, and where the code must release it. Check the error paths and the cancellation paths, not only the normal path.

## Integration and data

- Compare these items:
  - the contract versions and the schemas;
  - the feature flags and the environment variables;
  - the permissions;
  - the locale, the time zone, and the character set;
  - the numeric precision;
  - the cache state and the generated files.
- Capture the value at the entry and at the exit of the seam or the system boundary that fails.
- For a remote call, check the shape of the request and of the response. Also check the timeout, the retry, a partial failure, and if the call is idempotent.
- For a data fault, check the shape of the data at its entry and before the first wrong state.

## An incident

If harm continues in a real system, propose a reversible action to contain it. Examples are a rollback and a feature flag. On a real system, the action is a write, so it is stop reason 2. Only when the words of the user name the action, do it. Keep the evidence. Then continue the diagnosis.
