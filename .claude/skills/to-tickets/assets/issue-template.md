# Issue templates

Fill the template that fits. Remove each instruction in square brackets. If no written design exists, remove each part that names it. Write in English as `ste.md` says. Use the words of `GLOSSARY.md`. Respect the accepted ADRs.

An issue holds no copy of a scenario or a design, no source path, and no code. Paths of documents are allowed. If a snippet from a prototype encodes a decision better than prose, it is allowed.

A rerun uses the headings `## Scenarios` and `## What to change` as markers. Keep them.

## The breakdown

Show this at the stop. Do not publish it. The titles in the table stay in English, because they are the titles of the issues. The other cells, the notes, and the questions are in Chinese. The drafts stay in English.

| Draft ID | Title | Blocked by, with the reason | Delivers | Scenarios | Label |
|---|---|---|---|---|---|
| [A] | [title] | [none, or the draft IDs with one reason each] | [the behavior, in one line] | [each scenario as `file: title`, with its FR IDs] | [label] |

Under the table, give:

- the first frontier;
- the scenarios not sliced, each with its reason;
- the action: the number of issues, and the repository;
- the effects: who sees the issues, and which feature files get tags;
- the recovery step, which has two actions:
  - close each created issue with a comment;
  - remove only the tags that this run added, with the copy that the run makes as the reference.

In the improve mode, the column "Scenarios" lists the moves. Leave out the effect on the feature files and the removal of the added tags.

Then ask three questions:

- Is the size of each slice right?
- Is each blocking edge right?
- Should any issue be merged or split?

## The parent issue

The title is the file name of the PRD without the extension. In the improve mode, it is the file name of the written design without the extension. The body only links to the documents.

```markdown
PRD: `docs/prd/[feature].md`
Written design: `docs/design/[name].md`
Feature files: `[feature directory]/`
```

In the improve mode, keep only the line of the written design.

Give the parent issue no label.

## A slice issue

The title names the behavior that the issue delivers.

```markdown
## Parent

#[number of the parent issue]

## What to build

[The behavior from end to end, from the view of the user. If the slice starts with a preparatory refactoring, name it as the first unit. Do not list layers.]

## Scenarios

The scenarios in `[feature directory]/` that carry the slice tag of this issue: `@issue-` and the number of this issue.

FRs: [FR-001, FR-002]. PRD: `docs/prd/[feature].md`.

## Design

`docs/design/[name].md`: [the headings of the sections that this issue needs].

## Done when

- Each scenario with the slice tag of this issue passes and has no `@wip`.
- Each check that passed at the start still passes.
```

## A refactoring issue

The title names the refactoring. The issue has no scenarios and no slice tag.

```markdown
## Parent

#[number of the parent issue]

## What to change

[The refactoring in plain words, from the written design. State that the behavior stays the same. For a removal issue, name the old interface. If the removal waits for a date, name the date.]

## Design

`docs/design/[name].md`: [the headings of the sections that this issue needs].

## Done when

- The change of "What to change" is in the code.
- Each check that passed at the start still passes.
- [The check that the written design names for the kept behavior] passes.
```

For a removal issue, add this last line to "Done when":

- A search for the old interface finds no caller and no definition.
