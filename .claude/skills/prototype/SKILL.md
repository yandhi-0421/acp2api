---
name: prototype
description: >-
  Build throwaway code that settles how a flow or a data model must behave, or how a page must look. It makes a logic page to click through, or UI variants of a page. Use for "prototype two layouts for this page", "does this state model feel right", or "let me click through this flow".
---

# Prototype

## Goal

Build the smallest runnable prototype that gives the user enough evidence for one design decision. Some decisions look right on paper and feel wrong in real cases. A user who does not develop cannot feel a flow from a description. A wrong decision costs less in throwaway code than in production code.

A prototype defines:

- **Question:** the decision that the trial must settle, in one sentence.
- **Evidence:** the state, the result, or the difference that can answer it.
- **Non-goals:** the work that the trial leaves out.

Show the question at the top of the prototype. Then a reader can check later that it answered the right question. Stop before the production implementation. Code that runs is not the verdict of the user, and the production implementation is separate work.

## Steps

At each step that names a principle, apply it there. Follow the report rule of [principles](../principles/SKILL.md).

1. **Frame.** In your reply, write the question, the evidence, and the non-goals. Choose one mode:
   - logic, for business rules, state changes, or data shapes: read [logic.md](references/logic.md);
   - UI, for layout, information hierarchy, or interaction: read [ui.md](references/ui.md).

   If both apply, start with the decision that constrains the other. If they are independent, follow the focus of the user. If the mode is not clear and the user is present, ask, because the wrong mode wastes the whole prototype. If the mode is still not clear and the user is absent, use the mode nearest to the target code. A backend module means logic, and a page means UI. State the assumption at the top of the prototype.

   When you ask about the mode, recommend the mode nearest to the target code. Unless the user asks for both, build one prototype. Name the other decision in the report as the next prototype. This step is complete when your reply holds the question, the evidence, the non-goals, and the mode.
2. **Isolate.** Save the output of `git status` and `git diff` outside the working tree of the user. Create the worktree as [worktree.md](references/worktree.md) says. Write and install nothing in the working tree of the user. In the worktree, put the prototype at the location that the user gives, or else near its target code. Unless the framework requires a name, put `prototype` in the name of each new file, folder, or route. This step is complete when the worktree exists.
3. **Build.** Build the prototype as the reference of its mode says. Obey the constraints below. This step is complete when each part that the reference lists exists.
4. **Check.** Run the prototype. Inspect it with the checks of its reference. Apply [prove-it-works](../principles/references/prove-it-works.md). Here it changes one thing: a prototype is unproven until you open it and run its cases.

   A check script is not a test. Keep it in a temporary directory outside the worktree. If a check fails, fix the prototype. Then run the check again.

   Commit the prototype on its branch. Compare `git status` and `git diff` of the user with the saved output. If they differ, name the cause of each difference. If your work caused a difference, stop, and tell the user.

   This step is complete when each check has its result or the reason why it could not run. The commit must exist, and each difference of the outputs must have a cause outside your work.
5. **Deliver.** Give the report of the section "Output". Then stop.

The important moments are "that should not be possible" or "I thought that X". They show the bugs in the idea. Do not defend the prototype. Name the rule of the idea that the moment exposes.

If the user asks for new actions, cases, or variants, add them. Then do steps 4 and 5 again.

## Completion conditions

- `Complete`: the prototype runs, it shows the evidence, it has no production side effect, and it is ready for the verdict.
- `Unverified`: a check still fails, or it could not run. Give the checks that ran, the reason for each other check, and the exact run instruction. If a check did not run, never claim that it passed. Do not cross a constraint or skip an approval to make a check run.

## Constraints

- **Dependencies.** Add no framework, no shared infrastructure, and no production dependency. If the question needs a new dependency, apply [choose-boring-technology](../principles/references/choose-boring-technology.md). Then ask the user.
- **Data.** Use fixtures and state in memory for external services and writes. Never connect to a production write path or a destructive operation. Apply [protect-secrets-and-personal-data](../principles/references/protect-secrets-and-personal-data.md): each fixture holds invented data.
- **Scope.** Add only the states, controls, and checks for errors that the question needs. Add nothing for a later need. Show each state or result that can change the decision. Only for the interaction under trial, get the details right. Keep the rest rough. Write no tests, because a prototype that needs tests is no longer a prototype.
- **Branch.** Never merge the branch of a prototype. Apply [guard-one-way-doors](../principles/references/guard-one-way-doors.md): only after a yes of the user, push the branch. Only when the user says so, remove the worktree and the branch.
- **Documents.** Write no PRD, written design, glossary entry, or ADR. The caller records the verdict in its own document.

## Output

Write the report in Chinese. Give the question, the mode, one exact open or run instruction, and the cases or variants to inspect. Give the decision that the user must make. Give the status, the path of the worktree, and the branch and the commit of the prototype. List each term that the labels needed and `GLOSSARY.md` lacks. Add the report lines of the principles that you read.

If no skill called this skill, name the document for the verdict. A product decision goes to the PRD (`to-prd`), also a layout. A module interface goes to the written design (`architect`), and a term or an ADR to `domain-modeling`. A snippet of the validated logic can go into the written design or an issue.

## Stop rules

- If the question is not about how something looks or behaves, build no prototype. Tell the user what fits. Speed needs a measurement or `debug`, and the feasibility of a library needs a probe or `research`. A first version of a tool that works is normal work.
- Start no later skill.
