# No Stops After Approval

After the approval of a design or a plan, do the reversible work without a stop. The same holds after the freeze of the acceptance contract. Stop to ask the user only for one of the four stop reasons below.

## Why

Each stop stalls the work until the user answers, so the user limits the speed of the work. The user supervises after the fact. Code changes are reversible and reviewable, so a wrong reversible decision costs less than a stop. The four stop reasons are the cases in which this is false:

- the decision changes what the user approved (stop reasons 1 and 4);
- the decision cannot be undone (stop reason 2);
- the work cannot continue without the user (stop reason 3).

## When it applies

It applies after the user approves a design or a plan, or after the freeze of the acceptance contract, until the approved work is complete. Apply it each time that you want to ask "should I do X?" about reversible work.

The acceptance contract is the Gherkin scenarios in the scope of the work, and each test file that can change their meaning or result. These test files are step definitions, fixtures, test data, fakes, hooks, and the configuration of the runner. If you cannot find out if a test file is in the contract, treat it as part of the contract. If no frozen scenario uses a line that a test file gains, the line is no change.

The user confirms the scenarios before the work. The freeze comes after that confirmation, when a workflow skill proves that the tests of the scenarios can fail. After the freeze, a change to the acceptance contract needs the approval of the user, but a scenario that passes can lose its run-control tag. The workflow skill that freezes the contract records the freeze where later sessions read it, for example in the repository. If scenarios exist in the scope, and you cannot find out if they are frozen, treat them as frozen. An absent record does not show that the scenarios are not frozen.

Product direction comes from the user, and the PRD holds it.

This principle does not apply before the approval. Before it, show the plan to the user. The user approves the plan and each of its steps. An approved plan is an approval.

A direct request from the user with a clear scope counts as an approved plan. For such a request, do not show a separate plan for confirmation. A request without a clear scope is not an approved plan.

## Pattern

### Between the stops

- Make reasonable decisions. Continue the work. Then show the result and the reason. Do not ask "should I do X?".
- An agent decision is a decision that you make where the approved design leaves a gap. Give each agent decision with its reason in the report, so that the user can correct it after the fact.
- Continue reversible actions without a stop. Examples are code changes, test runs, local commits in the approved issue, notes, and the split of a task.
- A draft follow-up issue is the title and the body of a proposed issue, written in the report. Do not create it during the work.
- If you see a problem outside the approved scope, record it as a draft follow-up issue. Then continue.
- At the end of the work, ask the user once if you can create the draft follow-up issues. An issue is an external publication, so this question is stop reason 2.
- If one question must wait for the user, continue the work that the question does not affect.

### The four stop reasons

1. A change to the acceptance contract after its freeze. This includes the deletion of one of its tests.
2. A one-way action or a one-way decision, as [guard-one-way-doors](guard-one-way-doors.md) defines it. That principle also says when an approval confirms a one-way door.
3. A blocker: the work cannot continue without something that only the user can give. Examples are an access right, a secret, and a choice between two requirements that conflict.
4. Work beyond the approval: the work needs something that the approval does not cover. Examples:
   - a change to the approved design, for example for a requirement that fits only as a bolt-on;
   - a dependency, a public interface, or a feature that the approved design does not name;
   - a product decision that the PRD does not make.

Do not make the scope smaller in silence. If you cannot do a part of the approved scope, that is a blocker or a change to the acceptance contract.

When you stop, name the stop reason in the question.

### A run without a user

This section applies to each question that a principle asks the user, also before an approval. If no user can answer during the run, record each question and its reason in the report. Halt the work that depends on the answer. Continue the other work.

A brief from an agent that works under an approval can carry that approval to the run. It carries the approval only for the scope of the brief, and only if the brief names the approval. A one-way door and the other stop reasons still halt the work that depends on them.

## Self-check

1. After the approval, does each question to the user name one of the four stop reasons? **Evidence:** each question, quoted, with its stop reason and the fact that makes the reason true.
2. Does the report show each agent decision with its reason? Map each change in the diff that the design does not name to an agent decision or a question. **Evidence:** the count of agent decisions in the report, and the count of questions to the user. Also each such change with its agent decision or its question.
3. Did a stop reason occur with no question? **Evidence:** the diff of the acceptance contract since the freeze, and the diff of the dependency manifest and the new public exports since the approval. Match each change to a question or to a line of the approved design. Two changes need no match: a run-control tag removal, and an added line that no frozen scenario uses.

## Signs that you skipped it

You stopped without a reason:

- After the approval, a question asks "should I...?", "do you want me to...?", or "shall I continue?" about reversible work.
- The work waits at the end of a step, but the approved plan names the next step.
- A question names no stop reason.
- A problem outside the scope stops the work.

You did not stop for a reason:

- A one-way action, for example a push or an external message, occurs without a confirmation, because "the design is approved".
- The diff adds a dependency, a public interface, or a feature that the approved design does not name, and no question asked for it.
- A scenario of the acceptance contract changed or disappeared after the freeze, except for a run-control tag removal, and no question asked for it.
- The report leaves out a part of the approved scope, and it gives no reason.

## Relations

- [guard-one-way-doors](guard-one-way-doors.md): it defines a one-way door, and the confirmation and the recovery plan that each one needs. Stop reason 2 uses that definition.
- [test-behavior-not-implementation](test-behavior-not-implementation.md): it holds the approval before the deletion of a test of the acceptance contract. That deletion is stop reason 1.
- [redesign-from-first-principles](redesign-from-first-principles.md): a requirement that fits only as a bolt-on sends the work back to the design step. That is stop reason 4.
- [choose-boring-technology](choose-boring-technology.md): it holds the checks of a new dependency, and their place in the question for stop reason 4.
- [sequence-verifiable-units](sequence-verifiable-units.md): it holds the scope discipline, so a problem outside the scope gets no fix in this issue. Here, the problem gets a draft follow-up issue, not a stop.
