# UI prototype

Use this mode for layout, information hierarchy, or interaction. Examples: "what should this page look like?", or "show me two layouts for the settings screen".

## Result

Show comparable variants on one route of the application. Keep the purpose, the data, and the application around the variants the same for each variant. Use the runtime, the router, and the design system of the application.

The prototype has these parts:

- the host route;
- the variants;
- the design question in the prototype controls;
- the switcher;
- the production gate.

Keep the language of the application in the variants. Write only the prototype controls in the language of the user. The prototype controls are the question, each assumption, and the switcher.

For a question about feel, get the transitions, the feedback, and the error states of the interaction under trial right.

If no application exists, put the variants and the switcher in one self-contained HTML file, as for a [logic prototype](logic.md). Then the host route, the production gate, and their checks do not apply. Write all labels in the language of the user.

## Choose the host

- When the interface belongs in a current page or flow, use that route. Keep its route parameters, its access control, and the layout around it. Replace only the part under review. Reason: on an empty route, without the real header, data, and density, each variant looks good. A new section of a current page also uses that page as its host.
- Only when no current route can host the trial, add a marked prototype route. Follow the pattern of routes of the project. Before you add it, check again that no current page can host it.
- If the application has no routes, switch the variants with a flag or a debug option that it already has. Then the host route, the query parameter, and the direct URLs do not apply. Give the flag value of each variant in place of its URL. If no such flag exists, ask the user.

## Define the variants

- Build the smallest set that represents the current alternatives. Use the count of the user. Else start with two. Only when a variant shows a separate structural choice, add it.
- Give each variant a stable key and a short name.
- Apply [design-it-twice](../../principles/references/design-it-twice.md) to the variants. Here it changes what a structure is: the layout, the information hierarchy, or the primary interaction. It also moves your recommendation to the time after the user saw each variant. A change of color, type, or copy alone does not make a variant. If two variants come out too similar, build one again. Name the structure that it must not use.
- When it keeps the behavior the same, share data adapters and basic controls. Do not share layout code that removes the structural difference. Example: a shared header is good, but a shared page layout makes all variants the same.
- Reuse local read-only data paths. Use fixtures for external reads and for writes in the prototype. Keep the authentication and access checks of the application. Feed them fixture responses in place of an external authentication service.
- If the trial is about a database, use a scratch database or a local file that only the prototype uses. Put `prototype` in its name. This is the only exception to the constraint "Data" of `SKILL.md`.

## Connect the variants

- Show the design question and each assumption in separate prototype controls, with the switcher. Put them at the top of the page, above the host layout. If they move a part of the host layout out of view, let the user collapse them. Do not make them part of a variant.
- Read the active key from the query parameter `variant`. If the route already uses that parameter, use `prototype-variant`. If the key is absent or invalid, use a valid default.
- Give a small switcher that only the prototype has. It keeps the route and the other query parameters when the user changes the variant. It shows the key and the name of the active variant. It must look different from the design under trial, so that nobody judges it as part of a variant.
- Let the left and right arrow keys change the variant. When an input field, a text area, or an editable element has the focus, do not take the arrow keys.
- Add the production gate, so that the prototype stays out of production. In a production build, a current route shows its original interface, and a prototype route is off or absent.

## Check and deliver

A new worktree has no installed dependencies and no ignored local files, for example `.env`. Install the dependencies with the command of the project, in the worktree. If the application needs a local file that git ignores, ask the user before you copy it. Such a file can hold secrets.

Run the application with its current command, from the worktree. If its port is in use, use another port. Do not stop a process that you did not start.

Open and reload a direct URL for each variant. Compare all variants with the same data and the same application around them. Confirm that each variant shows a distinct structural choice, and that no variant writes to production. Confirm that the production gate works.

Deliver the URLs of the variants, the differences to inspect, and the decision that the user must make. Give the run command with the path of the worktree and the port. Say if the server that you started still runs. After the user saw each variant, give your recommendation with its reason. Stop there.

The verdict often mixes the variants, for example "the header of B with the sidebar of C". That mix is the design that the user wants. Give that mix as the verdict.
