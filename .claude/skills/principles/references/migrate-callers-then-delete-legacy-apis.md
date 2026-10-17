# Migrate Callers Then Delete Legacy APIs

When a new interface replaces an old interface, move all callers and delete the old interface in one change. Add no temporary compatibility code, for example an adapter, an alias, or a re-export of the old name.

## Why

Two interfaces for one task make two paths to read, test, and fix. Agents copy the first way that they find, so each way gets new callers, and the cleanup never ends. Compatibility code that keeps each middle step stable becomes debt that lives long. The codebase then grows only by additions.

## When it applies

In the name of this principle, an API is the interface of a module in the project.

It applies when you add a new interface while callers of the old interface still exist, for example in a simplification or a refactoring. It also applies when the code has two ways to do one task. Many callers are not an exception: a lever moves them all in one verified unit.

It does not apply when the old path and the new path must exist together:

- An external user depends on the backward compatibility of the old interface. An external user is a caller outside the repository, for example a client of a published package or of a public HTTP API. The evidence for an external user is the published export or the public interface spec, as `file:line`.
- The project cannot take one coordinated change that breaks callers, for example when old and new versions run together during a deploy.

## Pattern

- Before the change, list each caller of the old interface with a search.
- Move all callers in one verified unit. If the callers are many, or the edit repeats, use a lever, for example a codemod.
- Do not keep the old interface because internal callers still use it. Move those callers.
- In the same change, delete the old interface and its aliases, re-exports, and wrappers. Also delete its test-only exports and globals, and each path that only the old interface used.
- Change the tests to assert the new contract.
- If the old path and the new path must exist together, use Parallel Change. Then the temporary adapter is an exception with a time limit: give it a removal issue or a removal date.
- When nothing needs the old path any more, remove it and the temporary migration code.

## Self-check

1. After the change, does a search for the old interface find no caller and no definition? **Evidence:** the search command and its output. Also the build or the type check after the deletion, with its result line. A hit can stay only in a historical record, for example a changelog or an old database migration.
2. If you used Parallel Change, which external user or deploy step needs the old path, and when does the old path go? **Evidence:** that user or step, and the removal issue or the `file:line` of the removal date.

## Signs that you skipped it

- Two ways to do one task: two functions, endpoints, or modules do the same task, and both have callers.
- The diff adds a new interface, and the old interface keeps its callers.
- An adapter, an alias, a wrapper, a re-export, or a deprecation note keeps the old name alive, and no removal date exists.
- New code calls the old interface after the new interface exists.
- Names with `V2`, `New`, or `Legacy` stand next to the old name.
- The plan moves internal callers over several issues, and the old interface lives between them.

## Relations

- [sequence-verifiable-units](sequence-verifiable-units.md): it holds the end state of each issue. Thus the move of the callers and the deletion of the old interface fit in one issue, not in phases with a broken middle.
- [build-the-lever](build-the-lever.md): it holds the tool that moves many callers in one pass.
- [test-behavior-not-implementation](test-behavior-not-implementation.md): it holds the deletion of the old tests that protect only the old implementation.
