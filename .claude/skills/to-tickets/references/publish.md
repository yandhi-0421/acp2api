# Publish the issues

Sections: before the stop, the tracker commands, a rerun, the details of the order, the read-back, the tag checks.

`<drafts>` is a new directory outside the repository for each run. It holds the draft files. Keep it after the run, because the recovery step uses its copy of the feature files.

## Before the stop

Do these checks at the end of step 4, so that the user sees the final text. After each revision at step 5, repeat them. The checks are:

- the secret scan;
- the labels;
- the use of `@wip` and `@issue-`;
- the cycle check of slicing.md;
- steps 4 to 7 of "A rerun".

### The secret scan

Apply [protect-secrets-and-personal-data](../../principles/references/protect-secrets-and-personal-data.md). Here it changes what you scan: the text of each draft file, because an issue is an external publication that a revert cannot undo. Remove personal data and internal names from the text. Use the secret scanner of the project. If it has none, run this command on `<drafts>`:

```sh
grep -rliE -e '-----BEGIN [A-Z ]*PRIVATE KEY' -e '(password|secret|token|api_key)[[:space:]]*[:=]' -e '(^|[^A-Za-z0-9_])(gh[pousr]_|github_pat_|AKIA|sk-)[A-Za-z0-9_-]{16,}' <drafts>
```

The command prints only the names of the files with a hit. The evidence is an empty output. Read each hit. If a hit is not a secret, reword the text. The command is a minimum, so also read each draft for personal data.

### The labels

Check that the labels of the drafts exist: `gh api --paginate 'repos/{owner}/{repo}/labels?per_page=100' --jq '.[].name'`. If a label is absent, say so at step 5. Recommend that the user creates it. Do not create a label. Do not publish before the labels exist.

### The names of the tags

In the first run of the feature mode, search the feature files and the runner configuration for `@wip`. Also search the feature files for `@issue-`. If the project already uses `@wip` or `@issue-` with another meaning, say so at step 5. Recommend that the user renames the other use. Do not publish before the answer. In a rerun, skip both searches, because the first run did them.

## The tracker commands

Use the commands of `docs/agents/issue-tracker.md` to create issues, make sub-issues, add blocking edges, and add labels. Also use them to find the parent issue, and to read the sub-issues and the blocking edges. That file also tells how to get the database ID of an issue. This file adds only the commands that the tracker file lacks. In them, `{owner}` and `{repo}` are placeholders that `gh` fills from the git remote.

Create each issue from its draft file. For each sub-issue, add its label at the creation with `--label "<label>"`.

## A rerun

Apply [make-operations-idempotent](../../principles/references/make-operations-idempotent.md). Here it changes what finds an earlier run: the slice tags, the exact titles, and the paths of the documents in the bodies.

A run is a rerun if the parent issue exists, so do step 1 of this list in each run. Before you fill the drafts, do steps 1 to 3 of the list. Then the drafts use the published issues. Do steps 4 to 7 as a check of "Before the stop". In the improve mode, do only steps 1 to 3 and 6.

1. Find the parent issue with the command of "Find the parent issue" in the tracker file. Its title is in the template "The parent issue". Its body names the PRD path, or the written design path in the improve mode.

   If the command prints two lines, use the open issue. If two open issues match, ask at step 5. If the parent issue is closed, report it. Ask at step 5.
2. Read the sub-issues, as "Read the sub-issues" in the tracker file says. This filter also prints the bodies:

   ```sh
   gh api --paginate repos/{owner}/{repo}/issues/<parent>/sub_issues --jq '.[] | "#\(.number) \(.state) \(.title)\n\(.body)\n"'
   ```
3. Find the orphans. An orphan is an open issue that is neither the parent issue nor a sub-issue. Its body names the PRD path or the written design path, and it has the heading `## Scenarios` or `## What to change`. Run this command for each of these paths that exists:

   ```sh
   gh api --paginate 'repos/{owner}/{repo}/issues?state=open&per_page=100' --jq '.[] | select(.pull_request == null and ((.body // "") | contains("<path>")) and ((.body // "") | (contains("## Scenarios") or contains("## What to change")))) | "\(.number)\t\(.title)"'
   ```

   Remove the sub-issues of step 2 from the output. Report each orphan. At step 5, recommend that it becomes a sub-issue of the parent issue. Then treat it as a sub-issue.
4. Compare the open sub-issues with the tags, as check 2 of the tag checks says. A refactoring issue is a sub-issue whose body has the heading `## What to change`. A slice issue has the heading `## Scenarios`. Skip each refactoring issue, because it has no scenarios.
5. An open slice issue without a tagged scenario stopped before its tags. Report it with the scenarios that you assign to it, because its body names none. At step 5, ask the user to confirm them.
6. A draft refactoring issue and a refactoring sub-issue that make the same change are one issue. Compare the change, not only the title. Use the published number and title. Do not create it again. Do not draft again the work of a published issue.
7. List the tagged scenarios that the PRD retired. Find each FR ID that command 2 of "FR tags" lists and command 1 lacks. For each ID, run this command, where `<id>` is the FR ID as command 2 prints it, for example `FR-004`:

   ```sh
   grep -rnE '@<id>([[:space:]]|$)' <dir> | grep '@issue-'
   ```

   Put the hits in the report. Do not change a closed issue.

In a rerun, an untagged scenario can join an open slice issue whose behavior it completes, or go into a new issue. Show the choice at step 5. A new issue that a published issue blocks uses the number of the published issue for its blocking edge.

For an issue that exists, do the parts of the per-issue order that are absent. Read its API edges and its `Blocked by` lines first, as "The read-back" says. Add only the blocking edges that both sources lack. Show each published issue with its blocking edges in the breakdown. Include it in the read-back.

## The details of the order

Step 6 of `SKILL.md` gives the order. These details complete it:

- Right after the parent issue exists, write its number into each draft. In a rerun, use the parent issue that step 1 of the rerun found.
- Create the issues in the order that `tsort` printed, so that each issue exists before the issues that it blocks.
- The slice tags come last, so they mark a complete issue. If the run stops before the tags, the rerun finds a slice issue without a tagged scenario.
- If the command for a blocking edge fails, put the line `Blocked by: #<number>` at the top of the draft. Write one such line for each issue that blocks the issue of the draft. Use the number of the blocking issue as `<number>`. Then run `gh issue edit <issue> --body-file <draft>`.
- In the feature mode, before the first tag, run the four coverage commands of tag check 3. Keep the output. Then copy the feature directory into `<drafts>`: `mkdir -p <drafts>/feature-copy && cp -R <dir>/. <drafts>/feature-copy/`.
- In the feature mode, change the tag lines in this way:
  1. To tag a scenario, make one edit. Append the slice tag at the end of its tag line, as [gherkin.md](../../to-feature/references/gherkin.md) says. If the line lacks `@wip`, append it in the same edit.
  2. After the last issue, add `@wip` alone to each scenario not sliced that lacks it. Do this also when the run creates no issue.
  3. Keep the line end of the file. Match the `Scenario` line also, so that the edit is unique.

The parent number and the `Blocked by` lines are the only changes to the confirmed text.

## The read-back

Apply [prove-it-works](../../principles/references/prove-it-works.md) to the tracker. Run the loop of "Read the blocking edges" in the tracker file. Then run this loop. It prints each sub-issue with its body:

```sh
subs=$(gh api --paginate 'repos/{owner}/{repo}/issues/<parent>/sub_issues' --jq '.[].number') || exit 1
for n in $(printf '%s\n' "$subs"); do
  printf '#%s\n' "$n"
  gh issue view "$n" --json state,title,labels,body || exit 1
done
```

Compare the output with the breakdown that the user saw. For each confirmed edge, require an API edge or its exact `Blocked by: #<number>` line in the body. The tracker file says how to report a text edge. Compare the body of each issue that this run created with its draft file: `gh issue view <n> --json body --jq .body | diff -B - <draft>`. Read the parent issue with `gh issue view <parent> --json title,body`.

## The tag checks

In the improve mode, no scenario exists, so the tag checks do not apply. The read-back is the proof.

In the feature mode, run these checks from the project root. `<dir>` is the feature directory of the PRD.

1. Print each scenario whose count of slice tags is not one or whose tag line lacks `@wip`. Print also each `Feature` or `Rule` line whose tag line above has `@wip` or a slice tag.

   ```sh
   find <dir> -name '*.feature' -exec awk '
   FNR==1{prev=""}
   /^[[:space:]]*(Feature|Rule):/ && prev ~ /(^|[[:space:]])@(wip|issue-[0-9]+)([[:space:]]|$)/ {print FILENAME ":" FNR ": tag above: " $0}
   /^[[:space:]]*(Scenario|Scenario Outline|Scenario Template|Example):/ {
     n=gsub(/@issue-[0-9]+/,"&",prev); w=(prev ~ /(^|[[:space:]])@wip([[:space:]]|$)/)
     if(n!=1||!w) print FILENAME ":" FNR ": " n " slice tags, wip=" w ": " $0
   }
   {prev=$0}' {} +
   ```

   Each printed line is a failure, except in these cases:
   - The scenario has no slice tag and `wip=1`, and the list of scenarios not sliced names it.
   - The scenario has one slice tag and `wip=0`, and this run did not edit its tag line. Do not add `@wip` back to such a scenario.
   - A tag on a `Feature` or a `Rule` line that the copy in `<drafts>` also has is a tag of the project. Report it. Do not remove it.
2. Print each tag number that is not a sub-issue of the parent, and each open sub-issue without a tagged scenario. The first of the last two commands must print nothing. The last command prints each open sub-issue without a tagged scenario. Only a refactoring issue may appear there. Each other number that it prints is a failure.

   ```sh
   t=$(mktemp -d)
   gh api --paginate repos/{owner}/{repo}/issues/<parent>/sub_issues --jq '.[] | "\(.number) \(.state)"' > "$t/subs"
   awk '{print $1}' "$t/subs" > "$t/all"
   awk '$2=="open"{print $1}' "$t/subs" > "$t/open"
   grep -rhoE '@issue-[0-9]+' <dir> | grep -oE '[0-9]+' | sort -u > "$t/tags"
   grep -vxFf "$t/all" "$t/tags"    # tags that name no sub-issue
   grep -vxFf "$t/tags" "$t/open"   # open sub-issues without a tagged scenario
   ```
3. Run the four coverage commands of "FR tags" in [gherkin.md](../../to-feature/references/gherkin.md). The output must equal the output that you kept before the first tag. If the coverage failed before the first tag, report the failure with the owner `to-feature`. Do not repair it. A failure that is new after the tags is a defect of this run.
4. Compare the copy with `<dir>`. Print each changed line that differs by more than the added tags. The output must be empty. The `sed` command removes slice tags and `@wip` from tag lines in both trees. It keeps doc strings, other text, and CRLF line ends.

   ```sh
   t=$(mktemp -d)
   cp -R <drafts>/feature-copy "$t/before"
   cp -R <dir> "$t/after"
   find "$t" -name '*.feature' -exec sed -i.bak -E \
     -e '/^[[:space:]]*"""/,/^[[:space:]]*"""/b' \
     -e '/^[[:space:]]*```/,/^[[:space:]]*```/b' \
     -e '/^[[:space:]]*@/ { s/ @issue-[0-9]+([[:space:]]|$)/\1/g; s/ @wip([[:space:]]|$)/\1/g; }' {} +
   diff -rN -x '*.bak' "$t/before" "$t/after"
   ```
5. Print each slice tag with the `file: scenario` that carries it. In the breakdown, replace each draft ID with its issue number. Then compare the list with the column "Scenarios". Each difference is a failure. Only this check finds two tags that are swapped.

   ```sh
   find <dir> -name '*.feature' -exec awk 'FNR==1{prev=""} /^[[:space:]]*(Scenario|Scenario Outline|Scenario Template|Example):/{if(match(prev,/@issue-[0-9]+/)){sub(/^[[:space:]]+/,"");sub(/\r$/,"");print substr(prev,RSTART+7,RLENGTH-7) "\t" FILENAME ": " $0}} {prev=$0}' {} + | sort -n
   ```
