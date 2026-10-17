# Build

This file is the depth of step 6. Step 6 changes product code, and no contract file. For the removal of `@wip` in step 7, see the section "The freeze commit" of `freeze.md`.

Read the skill [tdd](../../tdd/SKILL.md). If you lack the Skill tool, read it as a file.

After each coherent change, commit it. Then run the scenarios of the issue with the command of "The run" in `evidence.md`.

If the written design names the seams, the user approved them. If no written design exists, the seams are agent decisions. In both cases `tdd` asks nothing about them.

The frozen scenarios are the outer loop of `tdd`, and a scenario that fails is the red of a cycle. The tests that `tdd` adds are outside the contract, in files that the freeze commit does not list. Apply [test-behavior-not-implementation](../../principles/references/test-behavior-not-implementation.md). Here it limits their number: each test that `tdd` adds must protect a rule that the scenarios do not reach. Do not copy a scenario to a lower layer.

If a check that passed at the start fails, or a scenario still fails after your change, apply [fix-root-causes](../../principles/references/fix-root-causes.md). Here it changes the first move: before the fix, use the scenario that fails as the reproduction check, or make one.

Apply [subtract-before-you-add](../../principles/references/subtract-before-you-add.md). Here it limits the change: add only what a scenario needs.

An added line in a listed test file passes the contract check. Read each added line. When a frozen scenario uses the line, it is stop reason 1.
