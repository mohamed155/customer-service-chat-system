# Contributing

Thanks for contributing to the AI Customer Service Platform. The rules every change
must follow live in the [project constitution](.specify/memory/constitution.md); this
page covers the day-to-day workflow.

## Workflow

1. **Start from a Linear issue.** Work is spec-driven: spec → clarify → plan → tasks →
   implement (see `specs/` and the constitution's *Development Workflow and Quality Gates*).
2. **Branch** from `master` using the Linear branch name (e.g.
   `mohamedahmedramadan154/moh-13-...`). Keep branches short-lived.
3. **Commit** with [Conventional Commits](https://www.conventionalcommits.org/)
   (`feat(auth): ...`, `fix(db): ...`, `docs: ...`).
4. **Run the checks** before pushing: format, lint, type check, build, and tests for the
   parts you touched. CI must be green before merge.
5. **Open a pull request.** The PR template
   ([`.github/pull_request_template.md`](.github/pull_request_template.md)) loads
   automatically.

## Pull request checklist

Every PR description includes the review checklist from the PR template. It mirrors the
constitution's Definition of Done, so reviewers and authors check the same things:

- Linked Linear issue
- Tenancy scope checked
- Permissions declared
- Input validated
- Errors mapped (RFC 9457, no internals leaked)
- Observability added (logs, traces, metrics)
- Tests added, including negative and cross-tenant isolation tests
- Docs and ADRs updated

Mark an item `N/A` with a one-line reason if it does not apply. At least one approval is
required, and every reviewer confirms constitution compliance before approving.

## Where to read more

- [Constitution](.specify/memory/constitution.md): principles, technology standards,
  Definition of Done
- [`frontend/CLAUDE.md`](frontend/CLAUDE.md): frontend conventions
- [`README.md`](README.md): local setup and quickstart
- [`docs/`](docs/): configuration, deployment, runbook, backup/restore
