# Revise a PRD

Change only what the new decision affects. Other documents refer to each ID, for example a Gherkin scenario. Thus, an ID must stay stable.

## Keep or retire an ID

- An FR or an SC survives if it keeps the same obligation or outcome. A decision can change its details.
- If a revision removes an item, or gives it a new meaning, retire its ID. Give the new meaning a new ID.
- If a revision splits one item or merges several items, retire each old ID. Give each replacement a new ID.
- Keep a retired item in its list as one line: the ID, `— Retired:`, then the replacement IDs or the reason. An example is `FR-004 — Retired: replaced by FR-007`.
- A deleted ID leaves no trace, so a later revision can reuse it by mistake.
