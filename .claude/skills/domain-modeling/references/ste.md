# ASD-STE100 for the documents of the workflow

This file is the single definition of the writing rules for the documents that the workflow writes in a project. It applies to each workflow skill, not only to domain-modeling. Before you write or change one of these documents, read it.

## Scope

Write each document of the workflow in English. Use ASD-STE100 (Simplified Technical English, STE) as its base register. The documents of the workflow include:

- the product requirements document (PRD);
- the Gherkin feature files;
- ADRs;
- `GLOSSARY.md` and `GLOSSARY-MAP.md`;
- issues and pull requests;
- any other document that a workflow skill writes.

The rules apply to the text of a document. In a Gherkin feature file, they apply to the text of each step, each title, and each description. The Gherkin keywords, for example `Scenario Outline` and `Given`, keep their form. The rules do not apply to code, identifiers, or code comments. They also do not apply to quoted data, for example a label that the product shows to its users.

The conversation with the user stays in Chinese. The "Chinese term" column of `GLOSSARY.md` connects the two languages. In the documents of the workflow, only quoted data and the "Chinese term" and "Avoid" columns of `GLOSSARY.md` and `GLOSSARY-MAP.md` contain Chinese words. This rule does not apply to the other files of the project.

## Rules

Obey these STE writing rules:

- Use one word for one meaning, and one meaning for one word. If `GLOSSARY.md` contains a word, use the word with the meaning that the glossary gives.
- Write no more than 20 words in an instruction. Write no more than 25 words in a descriptive sentence.
- Write only one instruction in each sentence. If two actions occur at the same time, you can write them in one sentence.
- Write instructions in the imperative form. Put a condition before its instruction, and put a comma after the condition.
- A restriction in an instruction, such as "only if", "only when", "unless", or "only after", is also a condition. Put it before its instruction. Do not change the restriction word. Write "Only when Y, do X." Do not write "Do X only when Y."
- Use the active voice. Use the passive voice only in descriptive text, and only when it is necessary.
- Do not make noun clusters of more than three words.
- Do not use the -ing form of a verb. The exception is a technical name.
- Do not omit words such as articles to make a sentence shorter. Do not use contractions.
- Write only one topic in each paragraph. Write no more than six sentences in a paragraph.
- To count the sentences in a paragraph, use these conventions:
  - A paragraph is a block of text between blank lines.
  - A table cell or a Gherkin step is a separate paragraph.
  - A list item with full sentences starts a new paragraph.
  - A bold name without a verb, such as "Design.", is not a sentence. A bold step name is a name, also when it is a verb, for example "Start.".
  - A lead-in sentence and its fragment items count as one sentence, also when a blank line separates them.
  - The word limits apply to the lead-in and to each item separately.
- Use a vertical list for complex text.

You cannot examine each word against the official STE dictionary. If you are not sure about a word, use the most common and simple word. Keep one meaning for it. Do not say that a word is "STE approved".

## Exception: technical names

Some established terms give strong knowledge recall. Keep the original words for these terms. Do not change them to plain STE words. Examples are the code smell names in *Refactoring* by Martin Fowler, such as Feature Envy and Shotgun Surgery.

The test: if a plain phrase replaces the term, can a reader still find the standard definition and the standard solution from the name? If not, keep the term. The reader can be a person or a model.

Some words reliably raise the intensity of a behavior, for example "relentless". The exception also applies to these words. The test: if a plain word replaces the word, does the behavior become weaker? If yes, keep the word.

If a document uses a name only to identify one product, file format, protocol, or tool, it is not a technical name. Examples are GitHub, Cucumber, and jq. Write each one as its owner writes it. Do not add these names to the "Technical names" section. If a document uses such a name for a concept with knowledge recall, it is a technical name. Examples are Gherkin, codemod, worktree, and REST.

For each term that you keep:

- Use the original spelling and capitalization.
- Do not translate the term. Do not use a synonym.
- Before you use the term for the first time, add it to the glossary:
  1. If the skill domain-modeling is not loaded, load it.
  2. Add the term to the "Technical names" section, with its source. [glossary-format.md](glossary-format.md) gives the location of that section.

STE also permits technical names that are not in its dictionary. Thus, this exception agrees with STE.
