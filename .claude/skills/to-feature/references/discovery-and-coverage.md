# Split and map the behavior

Use this guide to turn broad or uncertain input into small behaviors that are useful. All readers must have the same view of the behavior. Do not make a second requirements system.

## Split by observable outcome

A behavior, as [test-behavior-not-implementation](../../principles/references/test-behavior-not-implementation.md) defines it, gives one result that an actor or an external system can show alone.

Split a behavior when it contains separate:

- outcomes;
- actions or operations;
- actors with different rules;
- states that stand alone;
- rule branches with different results.

When the setup facts and the assertions support one result, keep them together. When a part has no useful result alone, stop the split.

Technical layers, ticket limits, and the order of implementation do not define a behavior. An interface defines a behavior only when it is the promised contract. A schema, an endpoint, or a screen can be such an interface.

A non-goal is scope information. Only when a non-goal defines an observable guardrail, write a scenario for it.

When several splits are valid, use this order:

1. the simplest valuable end-to-end case;
2. a workflow milestone with an observable state;
3. a distinct operation, for example create, view, amend, or cancel;
4. a rule variation, for example permission, eligibility, calculation, or exception;
5. a data variation that changes the result;
6. one actor, item, or channel before bulk, multi-party, or concurrent behavior.

Apply the order again after each split.

## Map rules to examples

This section applies Example Mapping. Its result is the example map of step 3 in `SKILL.md`.

For each behavior, write down:

- the rule that controls the result;
- one representative example, and one more example for each other equivalence partition or boundary value;
- each open decision that can change the result.

A rule states a decision, a permission, a calculation, a constraint, or a state change. An example gives the prior state, the event, the domain values, and the expected result. An open decision names the absent authority. It gives no answer.

When the clauses of a rule give independent results, split the rule. When unrelated events in an example show different rules, split the example. If no concrete example makes the result of a rule clear, the rule needs another product decision.

If you know the source that decides, or the decision owner, name it. The FR ID is the name. Keep these links as light notes. Use them to check the coverage. Do not return them as a deliverable.

## Cover the differences

Start with one representative example. Add another example only for a different result or a different equivalence partition. Do not add examples to fill a quota.

These are common kinds of equivalence partition:

- ownership or permission;
- allowed and forbidden state changes;
- exceptions, expiry, cancellation, or duplication;
- retry or event order;
- a promised dependency failure.

Only when a kind can change the result that the PRD states, look at it.

For a threshold, when each position has a different meaning, use values below, at, and above it. For "500 EUR or more", these values are 499 and 500. Apply the same test to time, locale, currency, round-off rules, and collection size.

The next rule applies to security, performance, accessibility, resilience, invalid input, and a dependency. Only when the PRD confirms such a behavior and it changes the observable acceptance, add an example for it.

If you find a product risk that the PRD does not cover, write it as an open decision. Do not make it an acceptance rule. Unless a source decides or the user confirms a decision, give it no expected result.

Many examples can show a compound rule. Many open decisions can show that the behavior is unsettled. Split the rule, narrow the behavior, or ask for the decision.

The PRD states the product result of each branch. This skill gives each stated branch an example. A branch that you find here, and that the FR does not state, is an open decision.
