# Verify the findings

Sections: the evidence gate, severity and proof, the filter, red runs and levers, deleted and weakened tests, the contract check, when to call blast-radius.

## The evidence gate

Only when a finding has all of these items, keep it:

- a stable location;
- a reachable path, or for an omission, the scope of the search;
- the failed behavior, project rule, or requirement;
- a concrete effect;
- a severity and a proof value;
- the smallest safe change that fits the repository.

Check each item in the source yourself. A finding of a view is a lead, also when two views give the same finding. Merge two findings that describe one defect, and name both views. If two views contradict each other, read the code. If the contradiction stays, report both.

## Severity and proof

The severity measures the effect if the finding is true. The proof value measures the strength of the evidence. These are two separate fields.

- `CRITICAL`: a credible exploit, access across a system boundary, a loss of data that you cannot reverse, an exposed secret, or a wide outage.
- `HIGH`: a material failure of correctness, security, a contract, or a migration. A fake answer on the path of an FR is usually `HIGH`.
- `MEDIUM`: a bug of limited extent, an important test gap, or a concrete cost of maintenance or performance.
- `LOW`: an improvement that you can act on, with a limited effect.

The proof value uses the proof levels of [prove-it-works](../../principles/references/prove-it-works.md):

- `confirmed`: the path and the effect each reach proof level 3 or higher, and no fact is unproven.
- `probable`: exactly one stated fact is unproven, and the other facts reach proof level 3 or higher.

Raise a fact at proof level 2 with a walk-through, or mark it unproven. A `probable` finding cannot block alone. A finding with two or more unproven facts is not a finding. Put each open fact that can change a finding under `Unknowns`. In the field "Evidence", give the proof level of each fact.

## The filter

Only for one of these reasons, drop a finding:

- its evidence fails the gate, for example two or more of its facts are unproven;
- it repeats a kept finding;
- the shared rules of [briefs.md](briefs.md) exclude it, for example taste or a format issue;
- a project rule or an established local pattern at the base allows it.

The session that wrote the code often runs this step. Any other reason would let the author filter the review. Only through a source that you can cite, use the context of the conversation. Such a source is the written design, an ADR, an issue, or a report. Unless its evidence fails, do not drop a security finding or a correctness finding.

Give each dropped finding with its reason, so that the user can see your judgment and override it. Do not cap the count of findings: a cap can hide a real `BLOCK`. If all kept findings are `LOW`, say that the change is probably sound.

## Red runs and levers

The report of the author and the report of `test-audit` give the red runs, the levers, and the mutation results. Mark each fact from them "reported". It is a lead until a run of this review proves it. A red that no run of this review proves stays "reported" under "Coverage and validation".

- Only when a finding or the decision depends on such a fact, run its check again in an isolated copy.
- To repeat the trusted red of a slice issue, run its scenarios at its oldest freeze commit in a worktree. A newer freeze commit can hold the product code. "The contract check" below gives the command. If the check of the oldest freeze commit prints a difference, its red proves only the old scenarios. The red of a changed scenario stays "reported".
- To repeat the red of another test, apply only the changes to test files to the base, as [guard-one-way-doors](../../principles/references/guard-one-way-doors.md) says.
- When the decision depends on the result of a check lever, run it again.
- Only in a worktree at the head, run a lever that edits files. A second run must change nothing.
- Name each of these gaps:
  - an absent report of the author or of `test-audit`;
  - a report without a red run or a lever.

A run of each red of each issue repeats the work of `bdd` and `test-audit`. Thus run only the checks that the decision needs.

## Deleted and weakened tests

Apply [test-behavior-not-implementation](../../principles/references/test-behavior-not-implementation.md) to each test that the scope deletes or weakens. Here it asks for the evidence of the last paragraph of its self-check. Look for that evidence in the scope, its commit bodies, and the report of `test-audit` or of the author. For a test of the acceptance contract, the evidence also includes the quote of the approval of the user. Without that evidence, the deletion or the change is a finding. Give each quote under "Coverage and validation", marked "reported", so that the user can confirm it.

## The contract check

On the integration branch, run the contract check of [freeze.md](../../bdd/references/freeze.md) for each issue with a freeze commit. "Find the freeze commit" there lists these issues, also a reopened issue.

The check compares the freeze commit with the working tree. Thus run it in a worktree at the head of the scope. There, find each freeze commit with the command for `HEAD`. For a scope of uncommitted work, run it in the working tree of the user, because the check writes nothing. For the staged changes only, run it in an isolated copy, as [scope.md](scope.md) says.

An issue can have two or more freeze commits. Then also run the check of its oldest freeze commit. Find that commit with this command:

```sh
git log HEAD --format=%H --grep='^Freeze the acceptance contract of #<n>$' | tail -n 1
```

Each difference that this check prints is a change of the acceptance contract after its first freeze. Apply "Deleted and weakened tests" to it. If only the quote of the approval is absent, mark the finding `probable`, because only the user knows the approval. A failure of the check of the newest freeze commit stays `confirmed`, as the next paragraph says.

A failure of the check of the newest freeze commit is a `confirmed` `HIGH` finding in the group "Requirements". The acceptance contract changed after its freeze. A quote of the approval of the user for that change can be in the report of `test-audit`, or in the body of the commit of the change. Then it is not a finding. Give the quote as "Deleted and weakened tests" says.

## When to call blast-radius

When one or more of these items is true, call the skill [blast-radius](../../blast-radius/SKILL.md). Each item is a search or a fact that the report can show.

1. The search of self-check 2 of guard-one-way-doors has a hit.
2. The diff changes a manifest, a lock file, a pinned version or a local patch of a dependency, a feature flag, or shared configuration.
3. The diff changes when code runs: async code, a lifecycle hook, teardown, a transaction, a retry, a timeout, a cache, or the order of events.
4. A changed or deleted symbol has users in another module outside the diff. Give the `git grep` command and its count of hits.
5. A view reports a possible effect outside the diff that it could not prove.
6. The user asks for it.

A change only to tests, documents, or comments is not risky alone. A change to a private function whose callers are all in the diff is not risky alone.

Call it from the main conversation, after the views. Give it the scope, the intent paragraph, the items that hit, and the levers of the report of the author. For item 6 alone, give it the whole scope. Its result changes the decision:

- If a safety fact is below proof level 4, or an unproven risk with a high cost stays, it gives `NEEDS DECISION`. The recommended answer is the run that would prove the fact. If no run can prove it, give the options, their effects, and a recommended option.
- Classify each risk of the result with "Severity and proof" above. Each kept risk is a finding in the group "Engineering risk".

If no item hits, the report says "blast-radius: not called", with the reason. Then give the door from self-check 2 of guard-one-way-doors. In place of the one-word blast radius, write `not measured`.
