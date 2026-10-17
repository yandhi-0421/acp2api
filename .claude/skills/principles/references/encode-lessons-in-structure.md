# Encode Lessons in Structure

Put a fix that repeats into a mechanism, not into a text instruction. Route each correction, and each error or result that you did not expect, to its destination as a lesson.

## Why

A reader must see, remember, and obey a text rule. A mechanism enforces the rule without the help of the reader. Agents also copy the code around them, so a weak guard becomes the template for the next change.

## When it applies

It applies when you write the same instruction a second time, or when a correction repeats. It also applies to each correction by the user, and to each error or result that you did not expect.

It does not apply to the first copy of an instruction that no error caused.

## Pattern

### Route each lesson

When the user corrects you or a check fails, decide if the lesson is a one-off or a pattern. Then route it by its scope:

| Scope | Test | Destination |
|---|---|---|
| A one-off | It applies only to this task. | the report |
| A preference of the user | It applies to all work with this user. | the memory of the agent, if the harness has one; if not, the report |
| A rule of the project | It applies to all work in this project. | `CLAUDE.md` of the project |
| A fix that repeats | The mistake occurred two or more times, and a mechanism can catch it. | a skill or a lint rule |
| A systemic problem | It applies to each project that uses the workflow. | a principle |

- Close the loop: apply the lesson now, or write a specific draft follow-up issue in the report.
- For `CLAUDE.md`, a workflow skill, or a principle, propose the text in the report. The deletion of an instruction there is also a proposal. Do not change the file yourself.

### Choose the mechanism

- At the second copy of an instruction, ask if a mechanism can do it. Examples are a lint rule, a flag, a runtime guard, and a script.
- If a mechanism can do it, put the rule into the mechanism. Then delete the instruction.
- Put a mechanism into the current issue only if the issue names it. If not, write it as a draft follow-up issue in the report.
- If the rule needs judgment, make the instruction more prominent. Add an example of the failure.
- Use the strongest mechanism that the case allows. From the strongest to the weakest:
  1. a state that the types cannot represent, so that wrong code does not compile;
  2. a lint rule or a banned API that fails CI;
  3. one shared function that all callers use;
  4. a runtime guard.

## Self-check

1. For each lesson, what is its destination: the report, memory, `CLAUDE.md`, a skill, a principle, or a mechanism? **Evidence:** each lesson with the path of its destination, its text in the report, or its draft follow-up issue.
2. For each instruction that you wrote a second time, which mechanism enforces it now? **Evidence:** the path of the mechanism and a run in which it fails on the wrong case, or its draft follow-up issue. If the rule needs judgment, quote the instruction and its example of the failure.

## Signs that you skipped it

- You acknowledge a correction, for example "I will keep that in mind", but you record nothing.
- You record a lesson, but you route it nowhere, so the mechanism never comes.
- You fix the one instance that the user named, and the pattern that makes it repeat stays.
- The user makes the same correction a second time.
- The same instruction stands in two files or in two prompts.
- A comment such as "always call X first" protects a rule that a type, a lint rule, or a check could enforce.
- An instruction stays next to the structural fix that already enforces it.

## Relations

- [make-illegal-states-unrepresentable](make-illegal-states-unrepresentable.md): it holds the strongest mechanism, a state that the types cannot represent.
- [build-the-lever](build-the-lever.md): it holds the tool for the work in front of you, which stays outside version control by default. A mechanism of this principle must last after the work, so it goes into the project through the current issue or a draft follow-up issue.
