# Issue tracker: GitHub

The issues and specs of this project are GitHub issues in `yandhi-0421/acp2api`. Use the `gh` CLI for all operations. `gh` finds the repository from `git remote -v`. If `git remote -v` names more than one GitHub repository, run `gh repo set-default yandhi-0421/acp2api` once in each clone.

## Conventions

- Create an issue: `gh issue create --title "..." --body-file <file>`. Write a long body to a file first.
- Read an issue: `gh issue view <number> --comments`.
- List issues: `gh issue list --state open --json number,title,labels`. Add `--label` to filter by label.
- Make an issue a sub-issue of a parent:
  - With `gh` 2.94 or later: `gh issue create --parent <parent> ...`, or `gh issue edit <parent> --add-sub-issue <child>`.
  - With an earlier `gh`: `gh api --method POST repos/yandhi-0421/acp2api/issues/<parent>/sub_issues -F sub_issue_id=<child-database-id>`. Get the database ID with `gh api repos/yandhi-0421/acp2api/issues/<child> --jq .id`. It is not the issue number.
- Add a blocking edge: `gh api --method POST repos/yandhi-0421/acp2api/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-database-id>`. If this fails, put `Blocked by: #<n>` at the top of the child body.
- Comment on an issue: `gh issue comment <number> --body "..."`.
- Add or remove a label: `gh issue edit <number> --add-label "..."` or `--remove-label "..."`.
- Close an issue: `gh issue close <number> --comment "..."`.

## Labels

The project uses two label names from the triage roles of Matt Pocock's skills. The triage skill is not installed.

- `ready-for-agent`: The issue is fully specified. An agent can do it without the user.
- `ready-for-human`: The issue needs the user, for example for a decision or a review.

The skill `wayfinder` uses five labels: one for its map, and one for each type of decision issue.

- `wayfinder:map`: The map of an effort that is too large for one session.
- `wayfinder:grilling`: A decision issue that a conversation with the user resolves.
- `wayfinder:prototype`: A decision issue that a prototype and the verdict of the user resolve.
- `wayfinder:research`: A decision issue that a search for facts resolves.
- `wayfinder:task`: A decision issue for work that must occur before a decision.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Run `gh issue view <number> --comments`.

## Find the parent issue

The parent issue of a PRD or of a written design has the exact title `<title>`. Its body names `<path>`, and it has no `## Parent` heading. This command lists the issues and does not search, because a search can miss an issue that is seconds old. It prints the number and the state of each match:

```sh
gh api --paginate 'repos/yandhi-0421/acp2api/issues?state=all&per_page=100' --jq '.[] | select(.pull_request == null and .title == "<title>" and ((.body // "") | contains("<path>")) and ((.body // "") | contains("## Parent") | not)) | "\(.number)\t\(.state)"'
```

## Read the sub-issues

This command prints the number, the state, and the title of each sub-issue of `<parent>`. For other fields, for example the labels or the body, change only the `--jq` filter.

```sh
gh api --paginate 'repos/yandhi-0421/acp2api/issues/<parent>/sub_issues' --jq '.[] | "\(.number)\t\(.state)\t\(.title)"'
```

## Read the blocking edges

This command prints the number of each issue that blocks issue `<n>`. A read failure does not prove an empty list:

```sh
gh api --paginate 'repos/yandhi-0421/acp2api/issues/<n>/dependencies/blocked_by' --jq '.[].number'
```

This loop prints the blocking edges of each sub-issue of `<parent>`. It prints `unverified` for a read that fails.

```sh
subs=$(gh api --paginate 'repos/yandhi-0421/acp2api/issues/<parent>/sub_issues' --jq '.[].number') || exit 1
for n in $(printf '%s\n' "$subs"); do
  printf '#%s blocked by: ' "$n"
  gh api --paginate "repos/yandhi-0421/acp2api/issues/$n/dependencies/blocked_by" --jq '[.[].number] | join(",")' || printf 'unverified'
  printf '\n'
done
```

A text edge is a line `Blocked by: #<number>` in the body of an issue, from the fallback of "Add a blocking edge". If only a text edge shows an edge, report it as a text edge, not as an API edge.
