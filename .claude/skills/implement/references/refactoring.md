# The refactoring issue

Sections: the rule, the steps, the output.

## The rule

The subagent follows this file for a refactoring issue. The skill `bdd` does not apply, because the issue has no scenarios. The issue keeps the behavior, as [test-behavior-not-implementation](../../principles/references/test-behavior-not-implementation.md) defines the word.

A preparatory refactoring and a refactoring issue are units of their own. The loop of `tdd` has no refactoring. Here each unit starts with a check that already passes.

## The steps

1. **Read.** Read the issue, the sections of the written design that it names, and the start file. This step is complete when you can name the behavior that the issue keeps and the check that proves it.
2. **Make the check.** Apply [sequence-verifiable-units](../../principles/references/sequence-verifiable-units.md). Here it applies to the first move. Make a repeatable check of the kept behavior. Run it before the move.

   If the code is not testable, make the small change that the design or the issue names. If neither names it, that change is stop reason 4. This step is complete when the check passes, and you know its command.
3. **Move.** Move in the shortest safe order: names, encapsulation, extract or move, move the callers, and remove the old path. After each move, run the check, and commit the move with `(#<n>)` at the end of its first line. The design gives the direction, and the code gives the evidence for small moves. A move follows each interface, seam, owner of state, dependency, and one-way decision that the design names. A move that needs a change to one of them is stop reason 4. Stop with `Blocked: design`.

   Apply [migrate-callers-then-delete-legacy-apis](../../principles/references/migrate-callers-then-delete-legacy-apis.md). Here it keeps the last two moves in this issue: all internal callers move, and the old interface goes. Only a removal issue deletes the old interface of a Parallel Change. Apply [subtract-before-you-add](../../principles/references/subtract-before-you-add.md): do no unrelated cleanup. Apply [build-the-lever](../../principles/references/build-the-lever.md): a codemod makes a repeated edit. This step is complete when the old path is gone, or the issue names why it stays.
4. **Prove.** Run all checks, as [sequence-verifiable-units](../../principles/references/sequence-verifiable-units.md) defines them. Run them from narrow to wide:
   - the focused tests;
   - the module tests;
   - the type check, the lint, the build, and the other static checks;
   - the broader regression tests.

   Run the "Done when" lines of the issue. Compare the failures with the start file. For a removal issue, search for the old interface. The search finds no caller and no definition. This step is complete when the last commit passes every check that passed at the start.

## The output

Write the report in Chinese. Give the issue, the files that changed, and each check with its command and result. State that the behavior is unchanged. Name each change of behavior as a defect. Give each agent decision and each draft follow-up issue. Stop after this issue.

If you cannot finish, report `Blocked`. Give its stop reason with `file:line`, and the smallest next action.
