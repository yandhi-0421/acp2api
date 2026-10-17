# Protect Secrets and Personal Data

Keep each secret out of code, commits, logs, test data, reports, and issue text. In each output that you show, redact secrets and personal data, and keep personal data to the minimum.

## Why

A secret in a commit, a log, or a public text spreads to copies that you cannot reach. Examples are the history, clones, log stores, and search indexes. A revert does not remove these copies. Thus the only recovery is to revoke the secret and make a new one. Personal data that leaks harms the people that it describes. Data that you do not keep cannot leak.

## When it applies

It applies to each output and each stored artifact. Examples are code, commits, configuration, logs, test data, fixtures, captured requests and responses, reports, issue text, comments, and searches on the web.

- A secret is a value that gives access. Examples are a password, an API key, a token, a private key, a session cookie, and a connection string with a password.
- Personal data is data about a person who can be identified. Examples are a name, an email address, a phone number, and an account ID.
- A public text is a text that people outside the team of the project can read.
- Internal names are the names of internal systems, hosts, customers, and people.

In a review, check the privacy risks at each system boundary: secrets, logs, data for other services, retention, and deletion.

It does not apply to made-up personal data in tests, for example `user@example.com`. A key for a test account is still a secret.

## Pattern

- Read each secret at run time from the secret source that the project names. Examples are the environment and a secret store. If the project names none, use the environment. Build commands, scripts, and checks on environment variables, so that the secret stays out of what you show.
- If the secret is not in that source, do not look for it in other places. It is a blocker: stop reason 3 of [no-stops-after-approval](no-stops-after-approval.md).
- Before you show a command, an output, or a captured artifact, write `<REDACTED>` in place of each secret.
- Captured requests and responses carry auth headers and cookies. Quote only the lines that carry the signal.
- Do not log secrets, personal data, or full customer payloads. Log only the fields that the diagnosis needs.
- Before a text goes anywhere public, remove all private content: secrets, personal data, and internal names. A search on the web is public too.
- In a finding or a report, do not copy a secret, personal data, or a usable attack payload. Give its location and a safe description.
- In fixtures and test data, use made-up values, and only the fields that the test needs.
- If the redacted output is not enough for the diagnosis, tell the user. Ask the user how to continue. Do not show the secret.
- If a secret reached a commit or a public text, tell the user at once. The secret must be revoked and replaced, because a revert does not remove it.

## Self-check

1. Scan the diff and the text for secrets. Do this before a commit, a report, an issue text, a pull request, or a comment. Use the secret scanner of the project. If it has none, search for these patterns:
   - private key headers;
   - the token prefixes of the services that the project uses;
   - values next to names such as `password`, `secret`, `token`, or `api_key`.

   Are the unredacted hits zero? **Evidence:** the scan command and its result line, with zero unredacted hits.
2. Which personal data do the new fixtures and log lines contain, and what needs each field? **Evidence:** each field with its `file:line` and the test or the diagnosis that needs it, or "none".

## Signs that you skipped it

In code or in a diff:

- A password, a key, or a token is a literal in code, in a tracked configuration file, or in a test.
- A `.env` file is in the diff, or `.gitignore` does not exclude it.
- A log line prints a full request, a header, a full customer payload, or a whole configuration object.
- A fixture holds real names, real email addresses, or a copy of production data.

In a transcript or a report:

- A command line shows a secret in clear text, for example a token in a `curl` header.
- A report or an issue quotes a captured request with its `Authorization` header or its cookies.
- A finding copies a secret or a usable attack payload, not its location.
- A search on the web contains an error text with user data or internal names.
- A secret reached a commit, and the only fix was a revert.

## Relations

- [guard-one-way-doors](guard-one-way-doors.md): a push or a publication that contains a secret is a one-way door. Thus run the secret scan before each one. The recovery step after a leak is to revoke and replace the secret.
