# Defaults when you do not wait for an answer

When "Stop rules" in `SKILL.md` says that you do not wait for an answer, use these defaults. They replace the questions of this skill. Do not ask about any of them. The section "A run without a user" of [no-stops-after-approval](../../principles/references/no-stops-after-approval.md) applies only to a stop reason, because the defaults answer the other questions.

- **A run without a user:** change `GLOSSARY.md` or the ADRs only when the task permits file changes. If not, list each proposed change in the report.
- **A conflict with the glossary, or a vague word:** keep the meaning that the text gives the word. The text is the document or the request that uses the word.
  - If the meaning is not clear, use the meaning that is more likely.
  - If no meaning is more likely, and the meanings give different behavior, it is stop reason 3 of no-stops-after-approval.
  - Use the glossary term of the meaning. If several terms fit, choose the most likely one.
  - If no term exists, choose a term. Then add it to the glossary.
- **A new entry in a run without a user:** propose a Chinese term for it. No user accepted this term yet. Thus, write "(not accepted)" after the Chinese term in its cell. A term with this mark can go into the glossary. Put the entry in the report.
- **An unclear bounded context:** choose the most likely one.
- **A conflict with an ADR:**
  - If the approved design names the conflict, follow the design.
  - If the approved design replaces the ADR, write the new ADR. Then mark the old ADR as superseded, as "Location and number" in [adr-format.md](adr-format.md) says.
  - If the design does not name the conflict, follow the ADR. If the work cannot follow the ADR, it is stop reason 4 of no-stops-after-approval.
- **A conflict with the code:** change neither the code nor the text. Put the conflict in the report. If the conflict changes a premise of the approved design, it is stop reason 4 of no-stops-after-approval. A behavior that the approved work adds or changes is not a conflict.
- **A glossary or ADRs in another language or format:** keep their form. Add each new entry with its English term. Put this fact in the report.
- **An ADR candidate:** do not offer it. List it in the report.
- **A product decision with no PRD:** put it in the report, as "Product decisions and ADRs" in [document-map.md](document-map.md) says.
- **A word in the frozen acceptance contract:** without the approval of the user, do not change its file. Give the word, its location, and the domain term in the report. If the meaning of the term changed, this use is stop reason 1, as "When a term changes" in [glossary-format.md](glossary-format.md) says.

## The report

Put each default that you used in the report. The section "The report rule" of [principles](../../principles/SKILL.md) defines the report. In a run without a user, the final message is the reply of the run, for example the reply of a subagent.
