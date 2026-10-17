# The conditions and the format of an ADR

## When to offer an ADR

An ADR holds only a technical decision. For the difference from a product decision, read "Product decisions and ADRs" in [the document map](document-map.md).

Only when all three conditions are true, offer an ADR:

1. **Hard to reverse.** A later change of mind costs much work. Each one-way decision of [guard-one-way-doors](../../principles/references/guard-one-way-doors.md) meets this condition. Use only the classification of that principle, not its rule for a door that you are not sure of. For a library, the test of about three months in "What qualifies" decides. The kinds in "What qualifies" below usually meet this condition, but examine the cost of each one.
2. **Unexpected without an explanation.** A future reader of the code will ask, "Why did they do it this way?"
3. **The result of a real trade-off.** Real alternatives existed, and the project chose one for specific reasons.

If one of the conditions is false, do not offer an ADR. When you offer one, give one short reason for each condition. If the user asks for an ADR and a condition is false, name that condition. Then let the user decide.

## Why each condition is necessary

- If a decision is easy to reverse, it needs no record, because the project can reverse it later.
- If a reader expects the decision, nobody will ask why.
- If no real alternative existed, there is nothing to record except "we did the obvious thing".

## What qualifies

These kinds of decisions usually meet the three conditions:

- **Architectural shape.** "We use a monorepo." "Orders use event sourcing, and Postgres holds a copy of the data for reads."
- **Integration patterns between bounded contexts.** "Ordering and Billing communicate through events, not through synchronous HTTP calls."
- **Technology choices that cause lock-in.** Examples are the database, the message bus, the authentication provider, and the deployment target. Do not record each library. Record only a library that needs about three months to replace.
- **Decisions about ownership and scope.** "The Customer bounded context owns the customer data. Other bounded contexts refer to it by ID only." A decision not to do something is as valuable as a decision to do it.
- **Deliberate deviations from the obvious path.** "We use manual SQL instead of an ORM because of X." This includes each decision where a reasonable reader assumes the opposite. The ADR prevents a "fix" of a deliberate choice.
- **Constraints that the code does not show.** "We cannot use AWS because of compliance requirements." "Each response must take less than 200 ms because of the API contract with the partner." If the constraint is a requirement of the PRD, the ADR records the technical decision that the constraint forces. It refers to the requirement.
- **Rejected alternatives, when the reason for the rejection is not obvious.** If the project considered GraphQL and chose REST for subtle reasons, record it. Without a record, somebody will propose GraphQL again in six months.

## Location and number

Put each ADR in a `docs/adr/` directory. If the project has several bounded contexts, each bounded context has its own `docs/adr/` directory. Put a decision about one bounded context into the directory of that bounded context. Put a decision about two or more bounded contexts, or about the full system, into `docs/adr/` at the root.

Name each ADR with a sequential number of four digits and a short title, for example `0001-use-a-monorepo.md`. To get the next number, find the highest number in the directory. Add one to it.

When a decision replaces an ADR, write a new ADR. Then set the status of the old ADR to `superseded by ADR-NNNN`, with the number of the new ADR. If the new ADR is in another directory, give its path in the status, for example `superseded by ADR-0002 (docs/adr/0002-use-one-queue.md)`. "Optional sections" below gives the form of the status field.

## Template

```md
# {Short title of the decision}

{One to three sentences: the situation, the decision, and the reason.}
```

That is the full template. An ADR can be one paragraph. Its value is the record that the project made a decision, and the reason for it. More sections do not give it more value.

If the ADRs of the project use another language or format, ask the user if you can convert them. When you do not wait for an answer, use the default in [defaults.md](defaults.md).

## Optional sections

Only when these sections give real value, add them. Most ADRs do not need them.

- **Status**: a `status` field in a YAML block at the top of the ADR. Its value is `proposed`, `accepted`, `deprecated`, or `superseded by ADR-NNNN`. When the project examines its decisions again, use it.
- **Considered options**: use it only when the rejected alternatives are worth a record.
- **Consequences**: use it only when an effect on other parts is not obvious and needs a statement.
