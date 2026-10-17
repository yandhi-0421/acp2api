# Logic prototype

Use this mode for business rules, state changes, or data shapes. These things can look right on paper and feel wrong in real cases. Examples: "does this state machine handle X, then Y?", or "can this data model hold this case?".

## Result

Create one self-contained HTML file that opens from the local file system. Put its HTML, CSS, JavaScript, and fixtures in that file. Then the user can open it with a double-click, and send it to another person.

Write the labels in the language of the user, with the domain terms of `GLOSSARY.md`. In Chinese, use the column "Chinese term". Then a person who does not develop can operate the prototype.

Show:

- the design question, at the top;
- the current relevant state;
- controls for the actions under trial;
- guided cases that can change the verdict;
- the result of the last action, with the reason of a rejection.

## Model the decision

Use the least complex model that keeps the behavior under review:

- For transformations of data without a current state, use pure functions.
- When actions change one current state, use a reducer.
- When each state permits different actions, use a state machine. Then the legal actions are part of the question.
- When its internal lifecycle is part of the question, use a small class or module.

Choose the shape that fits the question, not the shape that is easy to connect to the page.

Keep the model separate from the page. The model accepts data and returns data. It does not read or change the DOM. The page calls the model and shows its result. Reason: after the verdict, the validated model can go into the written design or an issue as a snippet. The page is throwaway.

If the rules already exist in the code, the model copies them by hand. Name the file and the commit that it copies, at the top of the prototype. If the copy holds uncommitted changes, say so.

Embed all fixtures, also for a question about a database. Do not add a framework, a package, a server, a build step, or a network request.

## Build the controls

- Show the state as labeled fields. A raw JSON dump can support the view, but it is not the main view.
- Give one control for each action that matters to the question.
- Keep each action available, so that the user can try an invalid order. Show if each action was accepted or rejected.
- Add only guided cases that can change the verdict. A normal case, a disputed edge, or a rejected action is often enough.
- Make each guided case a list of step buttons. Each button does one action. Start each guided case from a known state. Use the same model interface as the manual controls.
- Show the state after each action. When it helps the comparison, mark the last change.
- Use a plain, readable layout. Add no visual polish that does not support the trial, and no animation.

## Check and deliver

Open the file without a server. Run each guided case and one manual sequence of actions. After each step, confirm that the view shows the new state and the result of the action. After a rejected action, confirm that the state did not change. Confirm that the model has no dependency on the DOM.

Deliver the file path, the cases to inspect, and the decision that the user must make. Stop there.
