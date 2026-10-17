# The format of GLOSSARY.md and GLOSSARY-MAP.md

## GLOSSARY.md

Most projects have one bounded context, with one `GLOSSARY.md` at the root. Use this structure:

```md
# Glossary of {name of the bounded context}

{One or two sentences: what this bounded context is, and why it exists.}

Columns:

- Term: the word to use in the files of the project.
- Chinese term: the word to use in the conversation with the user.
- Definition: what the term is in this project.
- Avoid: the words not to use for this concept, in the files or in the conversation.

## {Name of a group of terms}

| Term | Chinese term | Definition | Avoid |
|---|---|---|---|
| Order | {Chinese term} | A request from a Customer for one or more products. | purchase, transaction |
| Invoice | {Chinese term} | A request for payment that goes to a Customer after delivery. | bill, payment request |
| Customer | {Chinese term} | A person or an organization that places Orders. | client, buyer, account |

## Technical names

The project approves these names as exceptions to ASD-STE100. This section gives no definitions, because each name already gives its standard definition.

| Name | Chinese term | Source | Avoid |
|---|---|---|---|
| {name} | {Chinese term} | {the book, the paper, or the project that defines the name} | {other words for this concept} |
```

The "Technical names" section of this template is only for a project with one bounded context. For several bounded contexts, see "GLOSSARY-MAP.md" below.

If the `GLOSSARY.md` of the project uses another language or format, ask the user if you can convert it. When you do not wait for an answer, use the default in [defaults.md](defaults.md).

## What goes into the glossary

- **Keep implementation details out.** The glossary is not a specification, a scratch pad, or a record of implementation decisions. It is a glossary only.
- **Add only domain terms: the terms that are specific to this project.** General programming concepts do not go into the glossary, also when the project uses them frequently. Examples are timeouts, error types, and utility patterns. Before you add a term, ask: is this concept specific to this project, or is it a general programming concept? Add only the first kind.
- **Technical names are the exception.** The "Technical names" section lists them, also when they are not specific to this project. To find which words are technical names, use [ste.md](ste.md). A general programming word never gets a row with a definition. If a document keeps it as a technical name, list it only under "Technical names".
- **The setup skill `/workflow-setup` writes the entries of the workflow terms.** A workflow term is a term that a principle or a workflow skill defines, for example "acceptance contract". The entries are between the markers `<!-- workflow-setup:start -->` and `<!-- workflow-setup:end -->`, in `GLOSSARY.md`, or in `GLOSSARY-MAP.md` with several bounded contexts. Each entry gives the fixed Chinese term. If a file defines the term, the definition names that file.
- **Do not change the text between the markers.** The setup skill owns it, and its next run shows a change there as a conflict. If the user wants another Chinese term, say that the change belongs in the source of the workflow. The section "Agent skills" of `CLAUDE.md` names it. A workflow term and its Avoid words apply only to the concept of their entry. If a domain concept needs the word of a workflow term, propose another word for it to the user.
- **A workflow term without an entry gets one at its first use in a document.** Add the entry outside the markers, in the same file and in the same form. Do not copy the content of the principle, because two copies can disagree. Say in the report that the setup skill lacks the entry. If an entry outside the markers has the concept of an entry between them, remove the outside entry.

## How to write an entry

- **Choose one word for each concept.** If several words exist, choose the best one. Put the others in the Avoid column.
- **Keep each definition tight:** one or two sentences. Say what the term is, not what it does. The definition holds only the meaning of the term. If an answer to an edge case limits what users can do, it is a product decision. Put it into its document from the [document map](document-map.md). For example, "an Order has at most 50 products" goes into the PRD, not into the definition of Order.
- **Give the Chinese term that the conversation uses.** If the conversation uses the English word, write the English word. Propose the term and its Chinese term to the user together. In a run without a user, a new Chinese term gets the mark "(not accepted)" (see [defaults.md](defaults.md)). When the user accepts the term, remove the mark.
- **Group the terms.** When natural groups appear, put each group under a subheading. If all terms belong to one group, use one table.
- **Keep a deprecated entry.** Do not delete it. Start its definition with "Deprecated: use {replacement term}." If no replacement exists, start it with "Deprecated: no replacement." Keep the rest of the old definition.

## When a term changes

In these cases, do the steps below:

- You add an entry or an Avoid word.
- The meaning of a term changes.
- The project stops the use of a term.

A rename stops the use of the old word. Thus, the old entry stays as a deprecated entry with the replacement, and the new term gets its own entry. Do not only change the Term cell.

The steps:

1. Add or update the entry.
2. Search the documents of the project for the term and for each Avoid word of the entry.
3. Update each use that stands for the concept of the entry, in the scope of the current work. The exceptions in "Completion conditions" of `SKILL.md` apply. In Chinese text, use the Chinese term.
4. List the other uses for the user.
5. If the meaning changed, examine each use. If a use needs the old meaning, give that meaning its own term, or list the use for the user. A use in a frozen acceptance contract is stop reason 1 of [no-stops-after-approval](../../principles/references/no-stops-after-approval.md).

If the request is the rename itself, the scope of step 3 is each document of the project. Also search the code. List each use in code for the user.

Without the approval of the user, do not change a file of a frozen acceptance contract. For the report about such a file, use [defaults.md](defaults.md).

## GLOSSARY-MAP.md

A project with several bounded contexts has a `GLOSSARY-MAP.md` at the root. Use it to find the bounded context of the topic and its glossary. If the bounded context is not clear, ask the user.

`GLOSSARY-MAP.md` lists the bounded contexts, the location of each one, and how they relate:

```md
# Glossaries of the bounded contexts

## Bounded contexts

- [Ordering](./src/ordering/GLOSSARY.md): receives and tracks the Orders of Customers.
- [Billing](./src/billing/GLOSSARY.md): makes Invoices and processes payments.
- [Fulfillment](./src/fulfillment/GLOSSARY.md): picks and ships the products in the warehouse.

## Relations

- **Ordering → Fulfillment**: Ordering sends `OrderPlaced` events. Fulfillment receives them and starts to pick the products.
- **Fulfillment → Billing**: Fulfillment sends `ShipmentDispatched` events. Billing receives them and makes Invoices.
- **Ordering ↔ Billing**: Ordering and Billing share the types `CustomerId` and `Money`.
```

With several bounded contexts, put the "Technical names" section into `GLOSSARY-MAP.md`, not into each glossary. A technical name is not specific to one bounded context. Use the form of that section in the template of `GLOSSARY.md`.
