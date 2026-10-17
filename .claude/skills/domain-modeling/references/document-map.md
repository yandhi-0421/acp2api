# Document map

This file is the single definition of the document map: the one document for each kind of fact in a project that uses the workflow. It applies to each workflow skill, not only to domain-modeling. Before you put a fact into a document, read it.

## The table

Each kind of fact has a single definition in one document. Other documents refer to that document, and they do not repeat the fact. A copy goes out of date, and then two documents disagree.

| Kind of fact | Document |
|---|---|
| Domain terms and technical names | `GLOSSARY.md`. With several bounded contexts, [glossary-format.md](glossary-format.md) gives the location of the technical names. |
| Bounded contexts and their relations | `GLOSSARY-MAP.md` |
| Product decisions and numbered requirements | the PRD in `docs/prd/<feature>.md`, one file for each feature |
| Technical decisions that meet the three ADR conditions | ADRs |
| Behavior | the Gherkin feature files, in the directory that the section `## The location of the Gherkin feature files` gives |
| Interface designs that the user approved | `docs/design/<name>.md`, one file for each feature, or for each opportunity that architect explores |
| Slices of work | issues |
| Decisions of an effort that is too large for one session | the closed decision issues of its map, until the document of its destination holds them. The skill wayfinder defines the map. |
| The freeze of the acceptance contract of one issue | the freeze commit. [freeze.md](../../bdd/references/freeze.md) defines it |
| Directories and kinds of documents | the section `## Directories` of `CLAUDE.md` |

The three ADR conditions are in [adr-format.md](adr-format.md). An ADR without a status field is accepted. The skill domain-modeling writes `GLOSSARY.md` and the ADRs. Before another workflow skill changes `GLOSSARY.md` or an ADR, load domain-modeling.

For the reason of a relation between bounded contexts, `GLOSSARY-MAP.md` refers to the ADR that gives it. It does not repeat the reason.

## Product decisions and ADRs

A product decision changes what the users can do. A technical decision changes how the system does it. A product decision goes into the PRD, also when it is hard to reverse. An ADR holds a technical decision. If a requirement of the PRD is the cause of the technical decision, the ADR refers to that requirement.

Example: "Accept only WeChat Pay" goes into the PRD. "Call WeChat Pay directly, without a payment aggregator" goes into an ADR that refers to that requirement.

The skill domain-modeling does not write the PRD. The skill to-prd writes it. If no PRD exists, domain-modeling puts the product decision in the report. It also tells the user that the decision belongs in the PRD.

A technical decision that does not meet the three ADR conditions gets no ADR. Before the build, the written design holds an approved decision about an interface. After the build, the code and its tests hold the decision.

The skill architect writes the written design. The skill domain-modeling does not.

## The location of the Gherkin feature files

Put the feature files of one PRD in a directory named `<feature>/`. The name is the file name of the PRD, `docs/prd/<feature>.md`, without the extension.

- If the project has a Gherkin runner, put `<feature>/` inside the feature-file directory of the runner. The configuration of the runner, or the default of the runner, names that directory.
- If the project has no Gherkin runner, put `<feature>/` inside `features/`.

Write one `.feature` file for each capability. Use the name of the capability for the file, for example `refund-approval.feature`.

## The section of CLAUDE.md

When a new top-level directory or a new kind of document appears, update the section `## Directories` of `CLAUDE.md`. If `CLAUDE.md` lists the directories under another heading, use that section. The section has one line for each top-level directory, with the kinds of documents in it. List only the directories that version control tracks. If `CLAUDE.md` has no such section, add one. If the project has no `CLAUDE.md`, create it with that section.
