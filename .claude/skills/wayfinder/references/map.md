# The map and the decision issues

Sections: the map, a decision issue, Not yet specified or an issue, out of scope, the changes after a resolution.

## The map

The map is one issue with the label `wayfinder:map`. Its title is `Map: <destination in a few words>`. Its decision issues are its sub-issues. [map-template.md](../assets/map-template.md) gives its body.

The map is an index, not a store. A decision has its single definition in its decision issue. The map gives the answer in one line and a link. It does not repeat the reasons. A copy goes out of date, and then the map and the issue disagree.

The map body does not list the open decision issues. A session finds them with the list of sub-issues, as tracker.md says. Thus a session reads the map body once. It reads the body of an issue only when the question needs it.

Each line of "Decisions so far" names one closed decision issue on the way to the destination. A research issue with an Unknown answer also gets its line. An issue that you close as "not planned" gets no line. For an answer under a delegation, write the line as `- Assumed: [<title>](<link>): <answer>`. Example:

```markdown
- [Store sessions in the database or in cookies?](https://github.com/<owner>/<repo>/issues/<n>): in the database, because the support team must end a session.
```

Write the map and the decision issues in English, as ste.md says. Give each issue a title that states its question, because the user reads the titles, not the numbers.

## A decision issue

[issue-template.md](../assets/issue-template.md) gives its body. Its title is the question in short form. It has one label for its type, as types.md says, and no other label. The answer is not part of the body. The resolution comment holds it.

One session must be able to answer the question. Thus the section "Read" names the documents, the issues, and the areas of code that the session reads. If the list is too long for one session, split the question. If you cannot split it, leave it in "Not yet specified" until an answer makes it smaller.

A blocking edge from issue A to issue B has one of two reasons. The answer of A changes the question of B, or it is an input to B. The order of work alone is not a reason for an edge.

## Not yet specified, or an issue

The map is incomplete on purpose. Do not chart what you cannot see yet. A plan for each part goes wrong when the first answers come.

The test is if you can state the question precisely now. The test is not if you can answer it now.

- **Issue.** The question is sharp, also when an edge blocks it. Example: "Which payment provider supports refunds after 90 days?"
- **Not yet specified.** You cannot state the question sharply yet. Write it as one item of that section, as loosely as you see it. Example: "Something about the refund rules for subscriptions, after the provider is clear."

Do not cut an item of "Not yet specified" into issues before an answer makes it sharp. One item can become several issues, or none. "Not yet specified" holds no decided question, no question of an open issue, and nothing out of scope.

When no decision issue is open, no answer can make an item sharper. Then make each item a decision issue or a line of "Out of scope". Do this also when one session can answer the item.

## Out of scope

The destination fixes the scope. Work beyond it is out of scope, not an item of "Not yet specified". Write it in "Out of scope", with one line of reason. It never becomes an issue of this map. It comes back only if the user changes the destination, and then as a new map.

If an open issue is beyond the destination, close it as tracker.md says. Add one line with its link and the reason to "Out of scope". Do not add it to "Decisions so far", because that section records the way that the effort took.

## The changes after a resolution

An answer can change the map in these ways. Draft each change.

- **A new question.** Draft a decision issue, with its blocking edges.
- **An item of "Not yet specified" that is now sharp.** Draft its decision issue. Remove the item, so that the question has one location only.
- **An issue that the answer makes wrong.** If its question is still sharp with a change, draft the new body. If the answer removes the question, close the issue as tracker.md says. Name the answer in the comment.
- **An issue beyond the destination.** Rule it out of scope, as "Out of scope" says.
- **No change.** Write "no change" for the issue or the item.

If an answer changes the destination itself, stop. Only the user changes the destination. If the destination changes before the map exists, return to step 2 of the chart. A new destination needs a new map, because the old questions served the old destination. The chart of the new map closes the old map. It adds that closure, with a link to the new map, to its list of step 5.
