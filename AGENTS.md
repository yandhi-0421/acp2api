# Instructions for Codex

The rules of this project are in `CLAUDE.md`. This file does not repeat them.

- Before each task, read `CLAUDE.md`.
- Obey each rule of `CLAUDE.md` as if this file contained it.
- Put each new project rule into `CLAUDE.md`, not into this file.

`CLAUDE.md` and the workflow skills use the words of Claude Code. Read them with these meanings:

- The workflow skills are plain files in `.claude/skills/`. Codex does not load them as skills.
- "Load the skill `<name>`" means: read `.claude/skills/<name>/SKILL.md`. Then follow it.
- When a skill names another skill, read the `SKILL.md` of that skill.
- If a step needs a feature that you do not have, for example a subagent, tell the user. Do not skip the step in silence.
