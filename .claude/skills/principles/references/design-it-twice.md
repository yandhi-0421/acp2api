# Design It Twice

If a design decision has no precedent in the codebase, make two or three designs that differ in structure. Then recommend one of them, from a side-by-side comparison, before you build.

## Why

Your first design idea is unlikely to be the best. A wrong build costs more than the exploration of two or three designs.

The structure of a design is its interface, the place of each seam, the owner of each piece of state, and its data model. After the build, callers and tests depend on the structure, so it is the most expensive part to change. A copy of the first design with small changes keeps that structure and its costs. Thus a comparison with such a copy shows only details. Only a design with a different structure shows what the first structure costs, or proves that it is better.

A list of designs without a recommendation gives the decision to the reader, who knows the code less well than you.

## When it applies

It applies when a decision has more than one viable approach, and the codebase has no precedent for it. A precedent is a solution in the codebase to the same kind of problem. Examples:

- a new UI interaction;
- an architecture decision, for example a new module interface or a new data model;
- a product design decision where the user experience depends on feel, not logic.

Confidence in your first idea is not a precedent and not a constraint.

It does not apply when:

- the work is mechanical, and the codebase already has the pattern. Follow that pattern.
- the work is a bug fix or a refactoring with a clear target state. If a real structural trade-off remains, compare two or three designs.
- the constraints allow only one viable approach. Name the constraint that excludes each other approach.

## Pattern

- Make each design concrete. For an interface, give its signature and a usage example: the code of one real caller.
- For a UI interaction that depends on feel, make each design a throwaway prototype. A sketch cannot show feel.
- Make each pair of designs differ in at least one part of the structure. A change of names, of parameter order, of a library behind the same interface, or of a flag does not make a new design.
- Give each design the best form that its structure allows. A design that is weak on purpose does not count.
- Show the designs to the reader one at a time. Then compare them in prose on depth, locality, and seam placement.
- Recommend one design, with the reasons. If parts of different designs combine well, propose a hybrid. Give a clear choice, not a menu.

Example: the first design is `exportReport(report, format)`, with a switch on `format`.

| Design | Is it a second design? |
|---|---|
| A class `ReportExporter` with the same switch in its `export` method. | No. One owner still knows all formats. |
| One module for each format, each with `render(report)`, and a registry that maps a format name to its module. | Yes. Each format owns its own knowledge. |
| The report module returns one neutral table, and each format only writes that table. | Yes. A new data model and a new seam sit between the report and the formats. |

## Self-check

Before you build, answer these questions.

1. Does the codebase have a precedent, or does a different exception apply? If an exception applies, skip the other questions. **Evidence:** the search command and its hits. For an exception, also its proof:
   - for a precedent, its `file:line`;
   - for a clear target state, the line of the issue, or a quote of the user request, that gives it;
   - for one viable approach, the constraint that excludes each other approach.
2. Does the written design show two or three designs, each with an interface and a usage example? **Evidence:** the path of the written design, or a quote from the report, with the line of each interface and of each usage example.
3. How does each design differ in structure from the others? **Evidence:** for each pair of designs, the part of the structure that differs. If only names, parameters, a library, or a flag differ, make a new design.
4. Which design do you recommend, and what does it cost? **Evidence:** a quote of the recommendation with its reason, and its main cost against the other designs.

## Signs that you skipped it

The design has no real alternative:

- The written design has one design, or its other designs have the structure of the first.
- The comparison names no case in which a different design is better.
- A design has no interface and no usage example, only words such as "simple" or "flexible".

The comparison gives no decision:

- The written design lists the designs, but it recommends none.

## Relations

- [deep-modules](deep-modules.md): it defines depth, locality, and seam. Use its terms in the comparison.
