# Subtract Before You Add

Before you add code, remove complexity from the code that the change touches. Then make the smallest change that solves the problem: add only what a requirement or a caller needs now.

## Why

An addition to a complex system compounds its complexity. A removal first leaves less code, shows the essential structure, and often makes the next design clear. Code-level decisions protect simplicity: aim for the most result with the least code and complexity.

Each line that nothing needs still costs a read, a test, and a review. Code for a future need also guesses the shape of that need, and the guess is often wrong. When the need comes, you know its shape, and the code is cheap to add then.

## When it applies

It applies when you add, refactor, or replace code, and when you judge the size of a diff. It also applies before you add an abstraction, a layer, an option, a guard, or a new path for a value through layers.

The code that the change touches is the functions and the types that the issue requires you to edit. Find this set before any removal. Other principles use this phrase with this meaning.

The smallest diff applies at the [build step](redesign-from-first-principles.md). At the design step, a redesign can change more than the smallest diff. The other parts of this principle still apply to it.

It does not apply when a new requirement fits only as a bolt-on at the build step. Then the smallest diff hides a wrong design.

## Pattern

### Remove first

- First remove the dead code, the redundant validators, and the stubs from the code that the change touches. [sequence-verifiable-units](sequence-verifiable-units.md) puts each such removal in its own unit, before the change.
- Code that an external user can call is not dead, also when it has no caller in the repository. [migrate-callers-then-delete-legacy-apis](migrate-callers-then-delete-legacy-apis.md) defines an external user. The removal of such code changes a published contract, which is a one-way decision of [guard-one-way-doors](guard-one-way-doors.md).
- When you refactor or improve code, look for removals before additions. Prefer a net decrease in lines of production code.
- If a file or a doc section has no new content, delete it. Do not leave a stub that only points elsewhere.
- In a prompt, remove each instruction that repeats another one, and each part of a template that no output needs.

### Add the minimum

- Make the smallest change that solves the problem. Fewer lines are better than elegant boilerplate.
- Do not break a contract near the change only to save lines: a caller, a test, or a public interface.
- Design for the usage that you observe now. Do not add a feature, an abstraction, a pattern, or a test for a later need.
- Add no validator, parser, or guard that the requirement does not demand. Validation at a system boundary is always necessary.
- If a hook, an option, or a layer in the diff has no requirement, remove it or inline it. When a real use exists, add it.
- Reach the minimum that works before you invest in polish, for example in configuration, generality, or speed.

### Simplify continually

- Leave the code that the change touches simpler and more capable, behind the same or a smaller interface. In a fix issue, [sequence-verifiable-units](sequence-verifiable-units.md) permits only removals here.
- Do not simplify other code in the same change. Record dead code outside the code that the change touches as a [draft follow-up issue](no-stops-after-approval.md) in the report.

## Self-check

Before you finish a change, or when you review a diff, answer these questions.

1. Which requirement or caller needs each new option, parameter, layer, or guard in the diff? A requirement is the issue, the acceptance contract, the approved design, or a question of another principle that the change must pass. If nothing needs an item, delete it. **Evidence:** for each item, the `file:line` of the requirement or the caller. For a question of another principle: the name of the principle, the question, and its answer with the evidence that the question names.
2. What did you remove before you added code? **Evidence:** the net change in lines of production code, for example from `git diff --numstat`, and the list of removals. For each removal, the search for its callers, with the scope of the search. Also the check of each source that a search can miss, as [prove-it-works](prove-it-works.md) says for an absence. Examples are configuration, routes, and entry points. If you removed nothing, give the search for callers of each function that the change touches, and its result.

## Signs that you skipped it

The diff adds what nothing needs:

- Speculative Generality.
- An option, a parameter, or a hook that no caller sets.
- A validator, a parser, or a guard that no requirement names.
- Work on configuration, generality, or speed before the feature works end to end.

The code keeps what has no use:

- Production code whose only callers are tests, and that no external user can call.
- A stub file, a doc section, or a prompt instruction that repeats content from elsewhere.
- A refactoring that adds lines of production code and removes none.

## Relations

- [redesign-from-first-principles](redesign-from-first-principles.md): it holds the design step, and the stop for a bolt-on at the build step. In that case, do not make the smallest diff.
- [boundary-discipline](boundary-discipline.md): it holds where and how to validate at a system boundary.
- [minimize-reader-load](minimize-reader-load.md): it prefers explicit code to clever code. Make a diff smaller by removal, not by denser code.
- [sequence-verifiable-units](sequence-verifiable-units.md): it holds the scope of one issue. Treat a simplification of other code as an unrelated problem.
- [no-stops-after-approval](no-stops-after-approval.md): it holds the draft follow-up issue for a problem outside the scope.
