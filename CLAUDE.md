# AI Customer Service Platform — Claude Code Context

The project rules, commands and pointers for every coding agent live in `AGENTS.md`, imported
below. Keep shared guidance there, not here, so all agents read the same rules.

@AGENTS.md

## Claude Code specifics

- Project skills are in `.claude/skills/`: the Spec Kit steps (`/speckit-specify`,
  `/speckit-clarify`, `/speckit-plan`, `/speckit-tasks`, `/speckit-implement`, …) and
  `/linear-task-pipeline` (Linear issue → PR → merge → Done).
- Frontend rules are in `frontend/CLAUDE.md`; Claude Code loads them when you work under
  `frontend/`.
