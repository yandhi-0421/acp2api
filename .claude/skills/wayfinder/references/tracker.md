# The commands on the issue tracker

Sections: before a chart, publish, the frontier, unfinished records, claim, record, close, the read-back.

Use the commands of `docs/agents/issue-tracker.md` to create issues, make sub-issues, add blocking edges, and comment. Also use them to read the sub-issues and the blocking edges. That file also tells how to get the database ID of an issue, and the fallbacks. This file adds only the commands that the tracker file lacks. In them, `{owner}` and `{repo}` are placeholders that `gh` fills from the git remote.

`<drafts>` is the directory of the drafts, outside the repository. Write each body and each comment to a file in `<drafts>` first. The shell changes a backtick or a `$` on the command line. End each file with a newline, because the read-back output ends with one.

## Before a chart

Check the five labels: `wayfinder:map`, `wayfinder:grilling`, `wayfinder:prototype`, `wayfinder:research`, and `wayfinder:task`.

```sh
gh label list --limit 1000 --json name --jq '.[].name'
```

If a label is absent, stop, as the stop rules of `SKILL.md` say. Do not create a label.

Look for an earlier map. This command lists each map with its state and title:

```sh
gh issue list --label wayfinder:map --state all --limit 200 --json number,state,title --jq '.[] | "\(.number)\t\(.state)\t\(.title)"'
```

If an open map matches the idea of the user, show it at step 2 of the chart. Do the same for an open map that the user names. Ask the user if the chart continues it, or if a new map replaces it. A closed earlier map does not stop a chart. Name it in the report.

- **Continue.** Keep its destination. Use the title and the body of each published issue as its draft. Draft only the questions that no published issue holds.
- **Replace.** List the closure of the old map at step 5. List the closure of each of its open decision issues too. Each closure comment links the new map. Close each one with the reason "not planned", right after you create the new map, before its decision issues.

## Publish

1. If the map does not exist, create it: `gh issue create --title "<title>" --label wayfinder:map --body-file <drafts>/map.md`. The command prints the URL. The number is its last part. If the map exists, start its draft from its published body. After step 3, write the changes as "Edit the map" says.
2. Create the decision issues in dependency order: an issue comes after each issue that blocks it. Between issues that do not block each other, use the order in which a session must take them. The order of the sub-issues is the order of the frontier.
3. Finish each issue before the next one. For one issue, the order is:
   1. create it with its label: `gh issue create --title "<title>" --label "wayfinder:<type>" --body-file <drafts>/<file>`;
   2. make it a sub-issue of the map;
   3. add each blocking edge from an issue that blocks it. If the command for an edge fails, add the line `Blocked by: #<n>` to the top of the draft first. Then write the draft with `gh issue edit <n> --body-file <drafts>/<file>`. This line is the exception to the rule "Names" of `SKILL.md`, and the yes of step 5 covers it.
   4. add each blocking edge from it to a published issue that it blocks. If the command fails, put the fallback line at the top of that issue, as in 3.3.

To change an open issue, write its new body: `gh issue edit <n> --body-file <drafts>/<file>`. Add `--title "<title>"` for a new title. Keep each `Blocked by:` line of the old body at the top of the new body.

In a rerun, first list the sub-issues of the map, as "Read the sub-issues" in the tracker file says. There, `<parent>` is `<map>`.

An issue with the exact title of a draft is published. Do not create it again. Do only its absent parts.

Also look for each open issue with the title of a draft and a label `wayfinder:<type>`. If it is not a sub-issue, make it a sub-issue. Do not create it again.

## The frontier

This loop prints each open decision issue of the map, in the order of the map. For each, it prints its title, its assignees, each `Blocked by:` line of its body, and the open issues of its blocking edges.

```sh
subs=$(gh api --paginate 'repos/{owner}/{repo}/issues/<map>/sub_issues' --jq '.[] | select(.state == "open") | .number') || exit 1
for n in $(printf '%s\n' "$subs"); do
  info=$(gh api "repos/{owner}/{repo}/issues/$n" --jq '.title + "\tassignees=" + ([.assignees[].login] | join(",")) + "\t" + ([(.body // "") | split("\n")[] | select(startswith("Blocked by:"))] | join(" "))') || exit 1
  open=$(gh api --paginate "repos/{owner}/{repo}/issues/$n/dependencies/blocked_by" --jq '.[] | select(.state == "open") | .number') || open="unknown"
  printf '#%s\t%s\tedges from open issues=%s\n' "$n" "$info" "$(printf '%s' "$open" | tr '\n' ' ')"
done
```

An issue is on the frontier when it has no assignee and no blocking edge from an open issue. For each number in a `Blocked by:` line, read its state: `gh issue view <k> --json state --jq .state`. If the state is `OPEN`, the line is a blocking edge from an open issue.

If the loop prints "edges from open issues=unknown", the API edges are unverified. Treat the issue as blocked, and say so. If the loop exits with an error, the frontier is unknown, not empty. Then stop, as the stop rules of `SKILL.md` say.

## Unfinished records

A record writes the line of the map last. Thus a decision issue that is closed as completed, but has no line in "Decisions so far", has an unfinished record. This command lists the closed decision issues:

```sh
gh api --paginate 'repos/{owner}/{repo}/issues/<map>/sub_issues' --jq '.[] | select(.state == "closed" and .state_reason == "completed") | "\(.number)\t\(.title)"'
```

For each, search the map body: `gh issue view <map> --json body --jq .body | command grep -c -F "/issues/<n>):"`. If the count is 0, read the resolution comment of the issue. It holds the answer and the changes to the map.

If it has no resolution comment, ask the user for the answer. Post it as its resolution comment first, as step 1 of "Record" says. A comment is not a change of a closed issue.

Another session can still write that record. Thus show the absent writes and the time of the closure to the user: `gh issue view <n> --json closedAt --jq .closedAt`. Ask the user to confirm that no other session still records the issue. After a yes, do the absent writes as steps 3 and 4 of "Record" say.

## Claim

Read the assignees just before the claim:

```sh
gh issue view <n> --json assignees --jq '[.assignees[].login] | join(",")'
```

If the command prints a name, take the next issue of the frontier. Exception: the user named this issue. If the named issue has an assignee or a blocking edge from an open issue, tell the user first. Wait for a yes. Then claim it:

```sh
gh issue edit <n> --add-assignee @me
```

In the work mode, also claim here each research issue of the frontier that you resolve, as types.md says. Run the first command again as the read-back. Two sessions of one user have the same login, so a claim cannot separate them. If both read the frontier before either claims, both take the same issue. This case is rare, and the skill accepts it.

## Record

Read the state of the issue: `gh issue view <n> --json state --jq .state`. If another session closed it, report it, and stop. Else, after the yes of the user, do these writes in this order:

1. Post the resolution comment: `gh issue comment <n> --body-file <drafts>/resolution-<n>.md`. For a comment in parts, post each part from its own file, in order.
2. Close the issue: `gh issue close <n>`.
3. Publish the new decision issues and the changed issues of step 4 of the work mode. Use "Publish" without its step 1. Close each issue that a draft closes, as "Close" says.
4. Make each change of the map body in one edit, as "Edit the map" says. The line of "Decisions so far" is one of them. It is the last write, so that a stop before it leaves a record that "Unfinished records" finds.

### Edit the map

Other sessions can edit the map at the same time. A write of the full body then removes the lines of the other session. Thus edit the map in this way:

1. Read the time of change first, and then the body:

   ```sh
   gh issue view <map> --json updatedAt --jq .updatedAt
   gh issue view <map> --json body --jq .body > <drafts>/map-now.md
   ```
2. In `<drafts>/map-now.md`, change only the lines of this session. Write each line that you add to `<drafts>/added.txt`, and each line that you remove to `<drafts>/removed.txt`. Write no empty line into these files, because it matches each empty line of the body.
3. Only if the time did not change, write the body. Replace `<time>` with the output of the first command of step 1:

   ```sh
   [ "$(gh issue view <map> --json updatedAt --jq .updatedAt)" = "<time>" ] && gh issue edit <map> --body-file <drafts>/map-now.md
   ```

   If the command exits with 1 and printed nothing, the time changed. Start again at step 1. A short gap stays between the check and the write. The skill accepts this rare case.
4. Read the body back. Each added line must be in it, and each removed line must be absent. `command` skips a shell function with the name `grep`, and `-x` compares whole lines. `tr` removes the carriage returns of a body that the web page changed.

   ```sh
   gh issue view <map> --json body --jq .body | tr -d '\r' > <drafts>/map-back.md
   command grep -c -x -F -v -f <drafts>/map-back.md <drafts>/added.txt
   command grep -c -x -F -f <drafts>/removed.txt <drafts>/map-back.md
   ```

   The second command counts the added lines that are absent from the body. The third command counts the removed lines that are still in it. Both counts must be 0. If you added no line, skip the second command. If you removed no line, skip the third command. If a count is not 0, start again at step 1.

## Close

Close an issue that leaves the scope, or that an answer makes wrong, with the reason "not planned". First write the comment to `<drafts>/close-<n>.md`. It names the reason, with the title and the link of the answer. Then post it, and close the issue:

```sh
gh issue comment <n> --body-file <drafts>/close-<n>.md
gh issue close <n> --reason "not planned"
```

After a yes of the user, close a complete map with `gh issue close <map>`. Its reason is then "completed".

Never delete an issue, and never change a closed issue.

## The read-back

After each write, read the issue back. Compare it with the text that the user confirmed.

- A body: `gh issue view <n> --json body --jq .body | diff -B - <drafts>/<file>`. The output must be empty.
- A title, a label, a state, and the assignees: `gh issue view <n> --json title,labels,state,stateReason,assignees`.
- The last comment: `gh issue view <n> --json comments --jq '.comments | last | .body' | diff -B - <drafts>/<file>`. For a comment in parts, compare the last comments with their files, in order.
- The sub-issues: the command of "Read the sub-issues" in the tracker file.
- The blocking edges: the command for one issue in "Read the blocking edges" of the tracker file. It also says how to report a text edge and a read failure.
