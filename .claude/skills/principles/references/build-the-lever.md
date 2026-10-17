# Build the Lever

When the work is not trivial, build the tool that does it or proves it. Do not do the work by hand.

## Why

A script does the work the same way each time, and it runs again at no cost. The tool is also the evidence: a reviewer can read it and run it again to check the work. A change by hand can be checked only if somebody does it again. The user cannot check hand work, but the user can see a command and its output.

## When it applies

A lever is a script or a tool that does the work or checks it.

It applies to all work that is not trivial: edits, migrations, analyses, and checks. Build the lever by default. The bar is triviality, not repetition: a task that occurs once also gets a lever when the lever makes the work checkable.

It does not apply to a few obvious edits that you can see at a glance.

## Pattern

- Do the first unit by hand to learn the recipe. Then build the tool. Run the tool on that unit. Compare its result with your result.
- Make the lever safe to run again.
- Build the smallest script that does or proves the job, never a framework.
- Examples of levers:
  - a codemod or a script for edits;
  - a generator for repeated files;
  - a SQLite query for an analysis;
  - a check that runs again.
- If one script can do all units in one pass, run it yourself. Do not give the units to subagents.
- When you give work to subagents, write one brief file with the recipe, the check, and the fences. The fences are the files that they must not change. Keep the brief outside their write scope, so that they cannot change the contract.
- Until the work merges, keep the lever outside version control. Put it in a directory that git ignores, or in the system temporary directory. Prefer the directory that git ignores, because the system can clear its temporary directory. Do not delete the lever before the merge, because a later reviewer runs it again.
- If the project has no directory that git ignores, add the directory of the lever to `.git/info/exclude`. Do not edit `.gitignore` for a lever, because that edit enters the diff.
- Commit a lever only when the design or the issue names it.
- A test of the project is not a lever. It goes into the project with the change.

## Self-check

1. Where is the lever? If you cite this principle and no script, generator, or subagent brief exists, you did not apply it. **Evidence:** the path of the lever, the command that runs it, and its output.
2. For a lever that does edits: does the lever give the same result as your first unit by hand? **Evidence:** the command that compares the two results, and its output.

## Signs that you skipped it

- The diff has many similar edits, and no script, codemod, or generator exists.
- The report cites this principle, but it gives no path to a lever.
- Subagents apply one recipe by hand, unit by unit, but one script could do all units.
- Subagents can change their own brief or check.
- The lever has options, plugins, or layers that the job does not use.
- The lever is gone before the merge, or it is in a commit that the design and the issue do not name.
- A second run of the lever changes the result again.

## Relations

- [prove-it-works](prove-it-works.md): it holds what a check must observe. This principle holds where the script stays.
- [encode-lessons-in-structure](encode-lessons-in-structure.md): it changes a lesson that repeats into a mechanism that stays in the project. A lever serves the work in front of you.
- [make-operations-idempotent](make-operations-idempotent.md): a lever is an operation too, so its self-check applies to the lever.
- [subtract-before-you-add](subtract-before-you-add.md): it holds the limit on additions. Here, it keeps the lever to the smallest script.
