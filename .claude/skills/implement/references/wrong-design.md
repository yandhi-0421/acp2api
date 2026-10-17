# A sign of a wrong design

Sections: the signal, the signs, the response.

The subagent reads the sections "The signal" and "The signs". The section "The response" is for `implement`.

## The signal

At the build step, a difference from the written design is a signal. It has three possible causes: the design is wrong, the design lacks a requirement, or the build goes too far. A subagent cannot ask the user, so it decides with evidence.

- A sign of a wrong design is stop reason 4. Stop that work. Return `Blocked: design` with each place as `file:line`.
- Any other difference is an agent decision. Record it with its reason in the report, and continue.

## The signs

Each principle holds its signs. Follow the rules of signs in [principles](../../principles/SKILL.md).

| Sign | Principle |
|---|---|
| The bolt-on signs | redesign-from-first-principles |
| A cast or an escape hatch | make-illegal-states-unrepresentable |
| A lock that serializes shared state | separate-before-serializing-shared-state |
| Callers that must know the internal rules | deep-modules |

A needed change to any of these is also a sign:

- an interface or a seam of the written design;
- an owner of state;
- a dependency;
- a one-way decision.

## The response

1. Put the question in the report and in a message to the user. Do not wait for the answer. Name stop reason 4. Recommend `architect`: it grounds again with the changes of the build.
2. Halt the issues that depend on the changed part. These are the issues that the failed issue blocks, and the issues whose `## Design` headings name the sections that the wrong design affects.
3. Keep the merged issues, and keep the issue branch of the failed issue. Continue the other issues.
4. After the user approves the new design, run `implement` again. The rerun finds the done issues in the log.
