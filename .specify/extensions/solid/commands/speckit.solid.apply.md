---
description: "Load and apply the SOLID/TDD/clean-code engineering discipline for the current implementation run"
---

## Steps

1. If a `solid` skill or equivalent tool is directly invocable in this environment (e.g. Claude Code's Skill tool), invoke it now and let it govern the rest of this run.
2. Otherwise, read `.agents/skills/solid/SKILL.md` in full — and any of its `references/*.md` files relevant to the task at hand — and hold yourself to the discipline it describes for the remainder of this run. That file is the canonical, agent-neutral copy; every coding agent working in this repository is expected to read it the same way.
3. This discipline governs **every** task executed in this run, not just the first one: Test-Driven Development (Red-Green-Refactor), SOLID principles, clean naming and structure, code-smell detection, and the pre/during/post-code checklists. Re-apply its checklists at each task boundary, not only once at the start.
4. Do not skip this for tasks that look small, mechanical, or "just plumbing" — those are exactly the tasks where undisciplined code accumulates.
5. This step does not replace or repeat any project-specific implementation steps — continue immediately to the next step of the command that triggered this hook once the discipline is loaded.
