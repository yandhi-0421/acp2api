---
name: domain-modeling
description: >-
  Sharpen the domain terms of a design or of the code in GLOSSARY.md, and record hard-to-reverse
  technical decisions as ADRs. Use for new or changed domain concepts, or to change GLOSSARY.md or
  an ADR: "is X the same as Y", "what should we call this concept", "add this to the glossary",
  "write an ADR for this".
---

# Domain Modeling

## Goal

Build and sharpen the domain model while you design with the user. The domain model is the vocabulary in `GLOSSARY.md` and the decisions in the ADRs. The result is one term for each meaning, in the conversation and in the documents.

If the task only reads the glossary, use it. Start no other action, because a read does not change the domain model. If the glossary has no entry, say so.

## During the conversation

Do these actions without a request:

- **Find the glossary of the topic.** If `GLOSSARY-MAP.md` is at the root, follow "GLOSSARY-MAP.md" in `references/glossary-format.md`. If not, use `GLOSSARY.md` at the root.
- **Challenge each word against the glossary.** Find each word of the user that conflicts with the glossary, is vague, or has two meanings.
- **Test the relations with concrete edge cases.** When the conversation discusses how concepts relate, invent edge cases. Make the user state where one concept ends and the other starts.
- **Compare the statements of the user with the code.** When the user says how the system works now, examine if the code agrees.
- **Compare each proposal with the ADRs of its bounded context and of the root.** If a proposal conflicts with an ADR, name the ADR. Propose to reopen the ADR only when the conflict has a real cost now, not only in theory.
- **Update the glossary at once.** Do not collect the changes for a batch. Update it in these cases:
  - A term is resolved.
  - A document uses a domain term or a technical name for the first time.
  - You and the user understand a word differently. First make the meaning clear.
  - The meaning of a term changes, or the project stops its use.
- **Only for a technical decision that meets all three conditions, offer an ADR:**
  - hard to reverse;
  - unexpected without an explanation;
  - the result of a real trade-off.

  Before you decide if a decision needs an ADR, or write or change one, read `references/adr-format.md`.

## Stop rules

A wrong word that goes silently into a document costs more than an interruption. A vague word does no harm until it goes into a document or into code. Thus:

- **A conflict with the glossary: say so at once, in one sentence that gives the recommended term.** An Avoid word that stands for the concept of its entry is a conflict too. Raise each word only once in a session. Example: "The glossary defines 'cancellation' as X, but you seem to mean Y, so I recommend 'Z'."
- **A vague word: before it goes into a document, code, or a design that the user approves, resolve it.** Propose one exact term. Example: "With 'account', do you mean the Customer or the User? They are different things." Do not stop the conversation for each vague word.
- **A conflict with the code: say so. Ask which is correct.** Example: "The code cancels a full Order, but you said that a partial cancellation is possible. Which is correct?"
- **A conflict with an ADR: say so at once.**
- **Do not wait for an answer in these cases:**
  - at the [build step](../principles/references/redesign-from-first-principles.md) of an approved design or of a frozen acceptance contract;
  - in a run without a user.

  A design discussion and a direct request to change the glossary or an ADR are not the build step. Use the defaults in `references/defaults.md`. Stop only for a stop reason of [no-stops-after-approval](../principles/references/no-stops-after-approval.md).

## Constraints

- **Only when you have something to write in a file, create that file.** When the first term is resolved, create `GLOSSARY.md`. Create `docs/adr/` for the first ADR.
- **Each kind of fact has one document, and other documents refer to it.** Before you put a fact into a document, read the document map in `references/document-map.md`. When a new top-level directory or a new kind of document appears, also read it.

## Output

- Before you create or change `GLOSSARY.md` or `GLOSSARY-MAP.md`, read `references/glossary-format.md`.
- Before you write or change a document, read `references/ste.md`.
- After each change to the glossary, tell the user each changed entry with its Chinese term.
- In the conversation, use the Chinese term of each glossary entry.

## Completion conditions

- A term is resolved when all are true:
  - the user accepts one meaning for it;
  - each edge case that you raised has one clear answer under that meaning;
  - each relation that the conversation discussed has at least one edge case.
- Before you end a turn that wrote or changed a document, examine the changed text. The first two checks below do not apply to `GLOSSARY.md`, `GLOSSARY-MAP.md`, code, quoted data, or the files of a frozen acceptance contract. The change is complete when:
  - each domain concept has its domain term;
  - no word from an Avoid column and no deprecated term stands for the concept of its entry. A text can name an old word on purpose, for example an ADR about a rename;
  - each fact is in its document from the document map;
  - the text obeys `references/ste.md`;
  - the glossary contains each term that the conversation resolved.
