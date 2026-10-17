# The stops

This file is the depth of the stop rules of `SKILL.md`. It names the cases of each stop and the way back.

## A change to the contract after the freeze

Stop reason 1 applies to each of these cases:

- A line of a contract file must change or disappear. This includes a step definition, a fixture, or a fake that a frozen scenario of any issue uses. An added line that no frozen scenario uses is not a change. Reason: a change there can break a slice that is done.
- You must delete a test of the contract.
- A contract check of a done issue fails.
- A scenario of the issue needs `@wip` again. The removal of `@wip` in step 7 is the only planned change.
- A scenario fails in step 6 because of a step definition, a fixture, the access, or a contract file that sets up the environment. Then a contract file must change.

A mismatch after a valid action and a valid assertion is not such a case. It is a defect in the product code.

In the question, give the smallest fix. After the approval, go to step 3 and repair the files. Go on through step 5. Make the new freeze commits as the section "The freeze commit" of `freeze.md` says. That section names who holds the approval.

## The owner of a blocked scenario

Halt only that scenario. Keep its `@wip`. Name the owner in the report:

- `to-feature` owns the text of the scenario: invalid Gherkin or results that conflict.
- A product choice that the PRD does not make goes to `grilling`, and then to `to-prd`.
- `architect` owns a scenario whose test cannot reach or observe its result.

After the freeze, a change to the scenario text is stop reason 1. A product choice is stop reason 4, also before the freeze. In both cases, only that scenario halts.

## A sign of a wrong design

At the build step, a requirement that fits only as a bolt-on is stop reason 4. Stop that work. Return `Blocked: design` with each place as `file:line`. Without `implement`, ask the user and recommend `architect`. When a subagent runs this skill, `implement` does this.

## A run without a user

Follow the section "A run without a user" of no-stops-after-approval. Here the report is the only channel: `implement` relays each question and its stop reason.

## An absent runner or test dependency

Stop reason 4 applies in the cases that `automate.md` names in its section "The runner". The question there gives the items of choose-boring-technology.
