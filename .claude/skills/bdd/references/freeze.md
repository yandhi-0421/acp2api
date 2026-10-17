# The freeze commit and the contract check

Sections: the freeze commit, find the freeze commit, the contract check.

This file is the single definition of the freeze commit. The skill `bdd` makes it. The skills `implement` and `test-audit` make a new one after an approved change. The skills `bdd`, `implement`, `test-audit`, and `code-review` check it, the skill `to-feature` finds it, and the skill `no-comments` reads its list of contract files.

## The freeze commit

The freeze commit records the freeze of the acceptance contract of one issue. The skill `bdd` makes it after the trusted red or the existing green of the issue. [evidence.md](evidence.md) defines both terms.

- The first line is exactly `Freeze the acceptance contract of #<n>`. `<n>` is the number of the issue.
- The body has the line `Contract files:`. After it, each line holds the path of one file and nothing else, relative to the root of the repository. The list ends at the first blank line. A trailer line, if any, follows that blank line.
- List each feature file with a scenario that carries the slice tag of the issue. List each test file of the acceptance contract. [no-stops-after-approval](../../principles/references/no-stops-after-approval.md) names the kinds.
- Do not list the dependency manifest or its lock file. A later change of a dependency must not fail the check.
- When the runner allows it, keep the configuration of the runner, with the exclusion rule, in its own file. A new file must not hide the settings that exist. If the settings are in the dependency manifest, write the rule there. The check does not cover that rule.
- If the runner must load a new path later, write it in a new listed file. A file pattern that the configuration already holds can load it too. Do not change a listed line for it.
- If no listed file changed, make the commit with `git commit --allow-empty -m <message> -- <listed paths>`.
- Run each commit command of this file from the root of the repository. If the commit holds a change of the dependency manifest or its lock file, add their paths after `--`. Do not put them in the list.

A full commit message looks as follows:

```text
Freeze the acceptance contract of #12

Contract files:
features/shop/cart.feature
features/steps/cart_steps.js
cucumber.js

Co-Authored-By: Name <name@example.com>
```

After the freeze, two changes to the listed files need no approval of the user:

- the removal of `@wip` from the tag lines of the scenarios of the issue, after they pass;
- added lines in a listed test file that no frozen scenario uses.

To remove `@wip`, remove the token and one space next to it. Delete a tag line that is then empty.

Any other change to what the check compares is stop reason 1 of [no-stops-after-approval](../../principles/references/no-stops-after-approval.md). The check compares each listed test file, and in each listed feature file the `Background` and the scenarios of the issue. This includes the addition of `@wip` again.

An approved change can make the contract check of a done issue fail for good. An example is a change to a listed line, a rename or a removal of a listed file, or a removal of a scenario. After such a change, make one new freeze commit for each done issue whose check then fails. Do these steps in order:

1. Commit the approved change first.
2. Read the differences that the check of each done issue prints. Each one must be the approved change. Any other difference is stop reason 1 of [no-stops-after-approval](../../principles/references/no-stops-after-approval.md).
3. Give the new freeze commit the first line of the earlier freeze commit of that issue. List the contract files as they are now. Replace each path that the change renamed or removed. Add each file that now holds a moved frozen line.
4. Commit with `git commit --allow-empty -m <message> -- <listed paths>`, on the current branch.

The approved change can remove each contract file of the issue. Then the issue has no acceptance contract. In the body of the new freeze commit, write the line `Contract files: none` in place of the list. Commit with `git commit --allow-empty --only -m <message>`, which commits no file.

`bdd` makes these commits under an approval in its brief or in the chat. `implement` and `test-audit` make them after they asked the user.

## Find the freeze commit

The skills `bdd`, `implement`, `test-audit`, and `code-review` take the newest freeze commit of issue #<n> that HEAD can reach. A freeze commit that HEAD cannot reach does not count. An example is a freeze commit on a branch that you renamed to end in `-old`.

```sh
git log HEAD -n 1 --format=%H --grep='^Freeze the acceptance contract of #<n>$'
```

The command prints the hash, or nothing. A second freeze after an approved change replaces the first.

This command lists each issue with a freeze commit that HEAD can reach:

```sh
git log HEAD --format=%s --grep='^Freeze the acceptance contract of #' | grep -oE '#[0-9]+$' | sort -u
```

The skill `to-feature` uses the same command with `--all` in place of `HEAD`. A wrong "frozen" only causes a question.

## The contract check

One command checks any freeze commit: the freeze of the issue itself, and each earlier freeze. Replace `<hash>` with the hash of the freeze commit, and `<n>` with the number of its issue.

The command works as follows:

- It runs from the root of the repository.
- It prints `FAIL` and the path of each listed file that the freeze commit does not hold.
- For a feature file, it compares each `Background` and the scenarios that carry the slice tag of the issue, at the freeze commit and now.
- For each other listed file, each line that the diff removes must come back as an added line of the same hunk. Added lines pass. This also covers a missing newline at the end of the file.
- It prints each difference. A difference in a feature file comes with three lines of context. The check passes only when it prints nothing and its exit status is 0.

Exception: a freeze commit with the line `Contract files: none` records an approved removal of the whole contract. Its check prints only `FAIL no listed path`, and that output is a pass.

For the comparison, a scenario is a block of lines:

- the tag lines above the scenario keyword (`Scenario`, `Scenario Outline`, `Scenario Template`, or `Example`), and each comment or blank line between them and the keyword;
- the keyword line;
- each line up to the next scenario keyword, `Rule`, `Background`, or the tag lines of the next scenario.

A `Background` is a block too, because it sets the prior state of each scenario. Tag lines above `Examples` belong to the block. Before the comparison, the command removes each `@wip` token from the tag lines, and it drops a tag line that is then empty. It ignores a trailing CR, trailing spaces, and trailing blank lines.

The check ignores `@wip`. The done check and the scope check of `implement`, and the "Done when" of the issue, find a removed or a returned `@wip`.

```sh
(cd "$(git rev-parse --show-toplevel)" || exit 1; h=<hash>; n=<n>
ext() { awk -v n="$n" '
function norm(s,   k, i, lead, o, tk) {
  sub(/[ \t\r]+$/, "", s)
  if (s !~ /^[ \t]*@/) return s
  match(s, /^[ \t]*/); lead = substr(s, 1, RLENGTH)
  k = split(substr(s, RLENGTH + 1), tk, /[ \t]+/); o = ""
  for (i = 1; i <= k; i++) if (tk[i] != "@wip") o = o (o == "" ? "" : " ") tk[i]
  return (o == "" ? "\001" : lead o)
}
function add(s) { if (own && s != "\001") b[++nb] = s }
function endb(  i) {
  if (own) { while (nb > 0 && b[nb] == "") nb--; for (i = 1; i <= nb; i++) print b[i] }
  own = nb = 0
}
ds { s = $0; sub(/[ \t\r]+$/, "", s); add(s); if ($0 ~ /^[ \t]*("""|[`][`][`])/) ds = 0; next }
/^[ \t]*@/ { tg[++np] = $0; next }
np > 0 && /^[ \t]*(#.*)?\r?$/ { tg[++np] = $0; next }
{
  kw = ($0 ~ /^[ \t]*(Scenario Outline|Scenario Template|Scenario|Example)[ \t]*:/)
  if (kw || $0 ~ /^[ \t]*(Rule|Background)[ \t]*:/) {
    endb()
    for (i = 1; kw && i <= np; i++) { split(tg[i], tk, /[ \t\r]+/); for (j in tk) if (tk[j] == ("@issue-" n)) own = 1 }
  }
  if ($0 ~ /^[ \t]*Background[ \t]*:/) own = 1
  for (i = 1; i <= np; i++) add(norm(tg[i]))
  np = 0; add(norm($0))
  if ($0 ~ /^[ \t]*("""|[`][`][`])/) ds = 1
}
END { endb() }'; }
pairs() { awk '
function flush(  i, ok) {
  ok = 1
  for (i = 1; i <= nr; i++) if (cnt[r[i]]-- <= 0) ok = 0
  if (!ok) printf "%s", t
  nr = 0; t = ""; delete cnt
}
/^diff --git/ { flush(); hd = 1; next }
hd && /^@@/ { hd = 0 }
hd { if ($0 !~ /^(index |--- |\+\+\+ )/) print "FAIL " $0; next }
/^@@/ { flush(); next }
/^-/ { r[++nr] = substr($0, 2); t = t $0 "\n"; next }
/^\+/ { cnt[substr($0, 2)]++; t = t $0 "\n"; next }
END { flush() }'; }
t=$(mktemp -d) || exit 1
list=$(git log -n 1 --format=%B "$h" | awk '/^Contract files:$/{m=1;next} /^$/{m=0} m')
{ if [ -n "$list" ]; then
    printf '%s\n' "$list" | while IFS= read -r p; do
      git cat-file -e "${h}:$p" 2>/dev/null || { printf 'FAIL not in the freeze commit: %s\n' "$p"; continue; }
      case $p in
        *.feature) git show "${h}:$p" | ext > "$t/a"; { ext < "$p"; } > "$t/b"; o=$(diff -U3 "$t/a" "$t/b" | tail -n +3) ;;
        *) o=$(git diff --no-color --no-ext-diff -U0 "$h" -- "$p" | pairs) ;;
      esac
      [ -z "$o" ] || printf 'FAIL %s\n%s\n' "$p" "$o"
    done
  else printf 'FAIL no listed path\n'; fi
  rm -rf "$t"
} | awk '{print} END{exit (NR > 0)}')
```
