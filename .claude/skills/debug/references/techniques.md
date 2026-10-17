# Techniques

This file is the depth of steps 2 to 4 of `debug`. Use the section that fits the failure. Choose the technique that removes the most doubt for the least work. Do not run every technique.

## Ways to make a reproduction check

Try the rows in this order. Take the first row that fits.

| Situation | Reproduction check |
|---|---|
| A test or a scenario already fails. | That test or scenario, with the command of a run that failed. |
| A function or a module fails. | A test that fails at the narrowest seam that still reaches the failure: a module test, an integration test, or an end-to-end test. |
| A service or an API fails. | A request script with an assertion on the status, the body, the headers, or the side effects. |
| A command line tool or a transform gives a wrong output. | A fixture input, and a diff of the output against a known-good output. |
| A path of the user interface fails. | A headless browser script that checks the DOM, the console, or the network result. |
| The failure does not occur locally. | A replay of a captured artifact through the code path alone. See "No local reproduction" in [kinds.md](kinds.md). |
| The code path needs much of the system. | A throwaway harness: the smallest part of the system that runs the code path with one call. |
| Some values of a large input space fail. | A property check or a fuzz check that records the seed of each failure. |
| Two versions or two configurations give different outputs. | The same input in both states, and a diff of the outputs. |
| A path becomes slow. | A benchmark with a constant workload, and a measurement of a good state. See "Performance" in kinds.md. |
| The failure appeared between two known states. | A bisect wrapper around the check of another row. See "Bisection" in kinds.md. |
| A person must act, for example click. | Numbered steps for the user. Each step asks the user to capture the time, the result, and the symptom. |

## Improve the check

When the first check works, improve it:

- Make it faster, for example with a cache of the setup, less start-up work, or a narrower test run.
- Make the signal sharper. Assert the exact symptom, not only "no crash".
- Make it deterministic, for example with a fake clock, seeded random numbers, an isolated file system, and no network.
- For an unstable bug, first record the seed and the time of each run that fails. A seed that you choose before a failure can hide the bug.

For a bug that fails on each run, a check that takes 30 seconds and fails at random is barely better than no check. A deterministic check of 2 seconds makes each later step fast.

## Reduce the search

- **delta debugging.** Remove parts of the input, the setup, or the event sequence. Keep each removal that keeps the failure. A cut in an isolated copy changes no file of the user.
- **A good state and a bad state.** Compare one dimension at a time. Start with the version, the configuration, the dependencies, the data, and the environment.
- **The state at each seam.** Inspect the state at the seams and at the system boundaries: modules, processes, services, and storage. Find the first place where the value becomes wrong.
- **Invariant checks.** State the condition that must stay true. Check it near each change of state, until the first violation shows.
- **Binary search.** If the failure has a good state and a bad state, search the history, the data, or the time range.
- **Known issues.** Search the local history and the documentation first. Then search the issue tracker of the dependency with the component, the version, and the class of the error. Treat a match as a hypothesis until the reproduction check confirms it.

Before a search on the web, apply [protect-secrets-and-personal-data](../../principles/references/protect-secrets-and-personal-data.md). Here it removes the private content from the text of the search.

For a regression in the code, read the history without a change of the working tree:

```bash
git log --oneline -- <path>
git diff <good>..<bad> -- <path>
```

## Probes

- Put each probe where two hypotheses predict different states.
- Use a debugger or a REPL first. Then use a trace point, a profiler, or a focused assertion.
- If the other tools cannot inspect a place, add a log there.
- Never log everything and then search the output. Broad logs hide the signal, and they can change the order of events.
- Change one variable for each probe. A probe that changes several variables cannot identify the cause.
- Tag each probe with the marker of this task, for example `[DEBUG-a4f2]`. Before the report, remove each probe. One search finds each probe. An untagged probe stays in the code.
- Before you add a log line, apply protect-secrets-and-personal-data. Here it limits the fields of the log line.
