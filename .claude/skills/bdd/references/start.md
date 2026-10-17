# The start

This file is the depth of step 2. Read the section "FR tags" of [gherkin.md](../../to-feature/references/gherkin.md) first.

## The list of failures

If the brief names a start file, use it as the list. If not, run all checks. Record each failure by name, and the count of tests of each check. Ignore the undefined steps of the scenarios with `@wip`.

Run the four FR coverage commands. The document map gives `<dir>`, and the issue gives `<prd>`. Keep the output of the commands. Steps 5 and 7 compare with it.

## The units

Do each unit that applies, in this order. Each unit has its own commit. After it, run all checks. The list of failures must stay the same.

1. The issue names a preparatory refactoring. Unless a commit of this issue already does it, do it first.
2. The test needs a seam that the product lacks. The seam is one that the written design names, or one that step 3 chose without a written design. Add the seam with no behavior: a call succeeds and returns the empty value that the step reads. It does not throw. Then the test reaches its assertion.

Step 3 or step 4 can find such a seam absent. Then come back to unit 2. The file `evidence.md` says why an error at the call is not a trusted red.
