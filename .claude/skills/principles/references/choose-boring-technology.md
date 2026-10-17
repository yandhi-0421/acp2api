# Choose Boring Technology

Solve the problem with the technology that the project already uses, and with the standard library of its language. Add a new dependency or other technology only in a design that the user approves.

## Why

Each new technology has a cost that lasts after the change. Somebody must learn it, install it, update it, and debug its failures. A project can pay only a few such costs. The failures of a technology that the project already uses are known. A new technology brings failures that nobody in the project knows yet.

A dependency is also code that nobody in the project reviewed, and it runs with the rights of the project. An install hook runs code on each machine that installs the package. The license and the maintenance of a dependency can change from one version to the next.

## When it applies

It applies when a change adds or replaces a technology. A technology here is a dependency, a framework, a language, a data store, a service, or a build or test tool. It also applies when a change upgrades a dependency, or changes a manifest or a lock file. In a review, it applies to each such change in the diff.

It does not apply to a program that stays outside the project, for example a command that you run once on your machine. If the program enters a manifest, a lock file, a CI file, or a script of the project, it applies.

## Pattern

- First, solve the problem without a new technology. Search the project for a dependency or a module that already does the job. Then look in the standard library.
- If a new technology is still necessary, it is a design decision. Put it into the design, with its reason. The user approves the design.
- After the approval, if the work needs a dependency that the design does not name, stop. In the question to the user, give the items below.
- If the new technology replaces an old one, put the removal of the old one into the same design.
- For each new dependency, record the items below in the design or the report. Take each fact from the registry or from the package files, not from memory.
  - the reason: what the technology of the project and the standard library cannot do;
  - the license, and whether it permits the use in this project;
  - the date of the last release;
  - the source: the registry, or a different source such as a git URL;
  - the install hooks: scripts that run when the package installs;
  - the runtime needs, for example a version of the language or a native build tool;
  - the release behavior: whether the dependency ships in the release, or only in development.
- A major-version upgrade is a replacement. It needs a design that the user approves.
- For each upgrade, check the license and the install hooks of the new version again.
- Add the dependency with the package manager, so that the manifest and the lock file agree. Then the lock file also holds the integrity data.

## Self-check

1. Can a technology that the project already uses, or the standard library, do the job? **Evidence:** the search command over the manifest and the imports, its hits, and for each hit the reason why it cannot do the job.
2. After an approval, does the approved design name each new dependency in the manifest diff? **Evidence:** for each new dependency, the line of the design that names it.
3. For each new dependency, what are the reason, the license, and the date of the last release? **Evidence:** the three items, each with the registry page, the package file, or the command that gives it. Also each install hook, and each source other than the registry, or "none". At the build step, also the line of the manifest diff.
4. Do the manifest and the lock file agree? **Evidence:** a locked install, for example `npm ci`, and its result line.

## Signs that you skipped it

A new technology does a job that the project can already do:

- The manifest diff adds a package for a job of the standard library. Examples are a deep-copy package instead of `structuredClone`, and an HTTP client instead of `fetch`.
- The diff adds a second technology for a job that the project already does. Examples are a second HTTP client, test framework, date library, or data store.

You did not check the dependency:

- The manifest changes, but the lock file does not. Or the lock file changes, and the report does not say why.
- A new dependency has an install hook, a source other than the registry, or no integrity data in the lock file. The design or the report does not name this.
- The report names a new dependency, but not its reason, its license, or its release date. Or it gives them with no source.
- The diff adds a dependency that the approved design does not name.

## Relations

- [no-stops-after-approval](no-stops-after-approval.md): it holds the stops after an approval. A dependency that the approved design does not name is one of them. This file holds the checks of the dependency.
- [migrate-callers-then-delete-legacy-apis](migrate-callers-then-delete-legacy-apis.md): a new technology that replaces an old one makes two ways to do one task. That principle moves all callers and deletes the old one in one change.
- [guard-one-way-doors](guard-one-way-doors.md): the replacement of a dependency that much of the code uses is a one-way decision. List it in the design as one.
- [prove-it-works](prove-it-works.md): it holds the rule to cite the source of a claim about a library. The license and the release date are such claims.
