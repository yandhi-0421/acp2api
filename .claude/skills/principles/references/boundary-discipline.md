# Boundary Discipline

At the system boundary, parse external data into a domain type, or return or raise the error. Inside the system, put business logic in pure functions that trust the domain types.

## Why

Validation in many places is noisy and redundant, and it gives a false sense of safety. Each guard expects a different guard to catch the bad input. A guard deep inside the system hides the crash that would show the real defect. One parse at the system boundary gives each later line a typed value that it can trust.

An error that the system boundary swallows does not disappear. It comes back later as a wrong value, far from its cause.

Logic outside the framework code is testable without the framework.

## When it applies

It applies when you add, change, or review validation, an error handler, a parser, or a framework adapter. It also applies when code reads external data.

A system boundary is a trust edge: the place where data enters the code that the project controls. Examples are CLI arguments, environment variables, configuration files, other files, the network, external APIs, the database, and the public interface of a published library. The network is a system boundary, also to a service that the project owns. Data from a system boundary is untyped until you parse it. This is also true for data that the project wrote earlier, when it comes back from a file or the database.

Validation at a system boundary is necessary. A guard inside the system that hides a crash is a symptom fix. To decide between the two, ask: does this data cross a system boundary at this line?

These limits apply:

- Do not extract a pure function only for testability. If the bugs are in how the callers use the function, the extraction hides them.
- Do not inject each dependency only to make mocks easy. Pass in the external dependencies at the system boundary.

It does not apply to data at a seam between modules of the project inside one process. Such a seam is not a system boundary, so do not validate that data again.

## Pattern

### At the system boundary

- Parse raw data into domain types. For example, validate the configuration in its parser, not inside business logic.
- Expect bad data and failures from each external source: absent fields, wrong types, timeouts, and partial responses.
- Return or raise each error, with the name of the input that failed and the reason. This also applies to an error that a catch block handles, and to an error after the last attempt of a retry.
- Do not swallow an error. Do not replace bad input with a default value.
- Pass the external dependencies in, for example a payment client or the clock. Do not create them inside the function. Write `processPayment(order, paymentClient)`, not a function that calls `new StripeClient(...)` inside.
- Give each external operation its own function: `api.getUser`, `api.getOrders`, and `api.createOrder`, not one `api.fetch(endpoint, options)`. Then each mock returns one shape, the test setup has no conditions, and each endpoint has its own types.
- Expose domain concepts, not the private representation of the system boundary. Do not export transport, storage, framework, or wire types through the public surface.

### Inside the system

- Trust the typed data. Do not validate it again.
- Put business logic in pure functions with no framework dependencies. Use Functional Core, Imperative Shell: the imperative shell only reads input, calls the pure functions, and writes output.
- Examples of pure functions:
  - a parser, from bytes to typed state;
  - a prompt builder, from state to a string;
  - a scorer, from state to a result.
- Keep general mechanism inside, and special policy at the edge. For example, the core returns a typed result, and the HTTP handler chooses the status code.

## Self-check

1. For each validation, null check, or guard in the diff: does the data cross a system boundary at this line? If not, apply these rules:
   - If the type allows the case, for example an absent key or an empty list, the guard is logic. Keep it.
   - If a parse at the system boundary excludes the bad case, remove the guard.
   - If no parse excludes the bad case, move the guard into the parse at the system boundary.
   - If the guard hides a crash, fix the cause of the crash instead.

   **Evidence:** for each guard, its `file:line` and the name of its system boundary. If the data does not cross a system boundary there, also the `file:line` of the parse that excludes the bad case. For a guard that is logic, the type that allows the case.
2. What does the caller see for bad input, and for a failure of the external source, at each system boundary that the change touches? A run against a real external service follows [guard-one-way-doors](guard-one-way-doors.md). **Evidence:** for each such system boundary, one run or test with bad input. If the source can fail, also one run or test with a failure of the source, for example a timeout or an error status. For each run, the command and the error line. A run that continues with a default value and no error fails this question.
3. Are the imperative shell and the logic separate?
   - Does the business logic import a framework, a network client, or a storage client? If yes, move that code to the imperative shell. Pass the dependency in.
   - Does a framework handler hold the branches of a domain rule? If yes, move the branches into a pure function.
   - **Evidence:** the search command for such imports in the files of the business logic, and the count of hits. Also the search command for such branches in the framework handlers, and the count of hits.
4. Does the public surface of a module at a system boundary export a transport, storage, framework, or wire type? **Evidence:** the search command over its exports, and the count of such types.

## Signs that you skipped it

Validation is in the wrong place:

- A null check deep in a call chain, for data that a system boundary already validated.
- Two or more places validate the same input.
- Business logic reads raw external data: an environment variable, a file, a raw request, or the result of `JSON.parse`.

The system boundary swallows errors:

- A catch block logs the error and continues, or it returns an empty or default value.
- A configuration value that is not valid falls back to a default with no error.

The imperative shell and the logic are mixed:

- A framework handler holds the branches of a domain rule.
- A function creates its own external client, for example `new StripeClient(...)` inside business logic.
- One generic fetcher has conditions, so each mock of it also needs conditions.
- The public surface exports a wire type, a database row type, or a framework request type. This is information leakage.

## Relations

- [fix-root-causes](fix-root-causes.md): it holds the rules for errors and guards inside the system. The question "does this data cross a system boundary at this line?" decides which of the two principles applies to a guard.
- [make-illegal-states-unrepresentable](make-illegal-states-unrepresentable.md): it holds the domain types that the parse produces, for example a branded type or a sum type.
- [test-behavior-not-implementation](test-behavior-not-implementation.md): it holds where to mock. The external dependencies that you pass in here are the places to mock.
- [protect-secrets-and-personal-data](protect-secrets-and-personal-data.md): an error message at a system boundary can quote the value of the input. That principle decides which values the message can show.
