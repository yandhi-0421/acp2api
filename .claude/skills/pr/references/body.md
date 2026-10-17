# The body

Sections: the template, the summary, the evidence, the Merge Danger, the issue lines, a request for one section.

Write in English STE. Use the terms of the `GLOSSARY.md` of the project. Do not write a preamble. Keep the prose short. Do not copy a report of the workflow or the report lines of the principles into the body. Write no local path in the body, for example a path in `<run>` or in a home directory.

## The template

```markdown
## Summary

<one or two sentences, and the smallest view of the change>

## Evidence

- **Before:** <a run, a test result, or a screenshot>
  **After:** <the run of all checks at the head>

## Merge Danger

**Door:** <one-way or two-way>

<the reason: each search with its hits, the one-way decision, or the review that gives the door>

**Blast Radius:** <local, internal, external, or not measured>

<what the change can reach outside the changed module>

**Review:** <the merge decision of code-review> at <the short hash of its head>

Closes #12
Closes #13
```

For an open pull request, the new body replaces the old body. Only when the user asks, keep a part of the old body that the template lacks.

## The summary

In the section `## Summary`, pick the smallest view that makes the main point clear. One view is often enough, and all views are almost never necessary. Put each view next to the sentence that it supports. Keep only the calls, the files, the states, and the seams that the point needs.

- **Pseudocode**: for logic or an algorithm.
- **A call tree**: for the control flow at run time.
- **A component tree**: for the structure of a user interface, with the state and the seams that matter.
- **A shallow file tree**: for the roles of files, or for a broad refactoring.
- **A Mermaid diagram**: for the interaction of components, the control flow, or the data flow.
- **A `diff` of one of these views**: when the point is what changes, and the shape around it exists. Match the shape of the diff to the topic.
- **The whole block of code**: when most of it is new, or when a cut would hide the owner or the order. Also when the reader needs a target shape to copy.

This example shows a `diff` of a call tree:

```diff
 submitForm
   createSession
     persistPrompt
+    expandSkillMention
     launchAgent
```

## The evidence

Show a before and an after for the main behavior.

- **After**: the run of all checks at the head, by this skill. Give the command and the result line of each check.
- **Before, in the workflow**: the trusted red of each done slice issue. Give the scenario, its failure line, and the freeze commit. Take them from the report of `implement` or of `code-review`.
- **Before, outside the workflow**: if a report or a run of this skill shows the result at the base, give it. Else write "Before: not run". Do not invent a result.

Find the commands of the checks in `CLAUDE.md` or in the CI configuration. If `git status --short` prints a line, the working tree is not the head. Then run the checks in a worktree at the head: `git worktree add --detach "<run>/copy" HEAD`. Make its dependencies with the commands of the project. After the run, remove it with `git worktree remove --force "<run>/copy"`.

Only when a run shows the failure, show the test that failed and now passes as pseudocode. In the workflow, the name of the scenario and its `Then` step do this. For a visible change, a screenshot is the best evidence. Only when the screenshot has an address that the body can show, use it.

## The Merge Danger

- **Door**: the door of the change, from [inputs.md](inputs.md). The push of `pr` is a one-way action, but it is not the door of the change.
- **Blast Radius**: the word of inputs.md. Under the word, name each place that the change can reach outside the changed module. Consider each possibility. Examples are a consumer of an interface, stored data, and a change of layout on a small screen.
- **Review**: the merge decision of the newest report that names the head, as inputs.md says, and the short hash of its head. If no report names the head, give the decision of the newest report. Add "no `code-review` ran at the head". Without a report, write only that.

## The issue lines

Write one line `Closes #<n>` for each issue of "The issues to close" in inputs.md. For an issue of another repository, write `Closes <owner>/<repo>#<n>`. The report of `implement` names the parent issue. If the list of issues lacks the parent, write `Part of #<parent>`.

GitHub closes these issues only when the pull request merges into the default branch. If the base branch is another branch, the question of step 3 says so.

## A request for one section

If the user asks for one section of an open pull request, change only that section. Keep each other part of the old title and body, except the issue lines.
