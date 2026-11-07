# Contributing

Thanks for contributing to the AI Customer Service Platform. The rules every change
must follow live in the [project constitution](.specify/memory/constitution.md); this
page covers the day-to-day workflow.

## Workflow

1. **Start from a Linear issue.** Work is spec-driven: spec → clarify → plan → tasks →
   implement (see `specs/` and the constitution's _Development Workflow and Quality Gates_).
2. **Branch** from `master` using the Linear branch name (see
   [Branch names](#branch-names)). Keep branches short-lived.
3. **Commit** with [Conventional Commits](#commit-messages)
   (`feat(auth): ...`, `fix(db): ...`, `docs: ...`).
4. **Run the checks** before pushing: format, lint, type check, build, and tests for the
   parts you touched. CI must be green before merge.
5. **Open a pull request.** The PR template
   ([`.github/pull_request_template.md`](.github/pull_request_template.md)) loads
   automatically.

## Branch names

Use the branch name Linear generates for the issue (the issue's **Copy git branch name** action),
for example `mohamedahmedramadan154/moh-20-commit-conventions-commitlint`. It contains the
issue ID (`moh-20`), which is what links the branch and its pull request to the Linear issue
automatically. Don't rename it. Work that has no Linear issue doesn't start until one exists.

## Commit messages

Commits **and pull request titles** follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <summary>

<optional body: what and why>

Refs MOH-20
```

- **Types:** `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `style`, `revert`.
- **Scope** is optional, lowercase, and one of the names in
  [`commitlint.config.mjs`](commitlint.config.mjs): the backend modules
  (`conversations`, `knowledge`, `tenancy`, …), shared crates (`db`, `kernel`, …), the frontend
  (`frontend`, `dashboard`, `widget`, `web`) and cross-cutting areas (`infra`, `ci`, `adr`, `deps`, …).
  Add a new scope to that file in the PR that introduces the module.
- Header is at most 100 characters. Merge commits are exempt.
- A `!` after the type or scope (`feat(api)!:`) marks a breaking change.

### Checks

- **Locally:** `pnpm install` in `frontend/` installs a `commit-msg` hook (lefthook) that runs
  commitlint on every commit. Without it, run
  `pnpm --dir frontend exec lefthook install`.
- **In CI:** the _Commitlint_ workflow checks the PR title and every commit in the PR.

## Merging

`master` is protected by a repository ruleset:

- A pull request is required. Direct pushes, force pushes and branch deletion are blocked.
- The checks `verify`, `real-backend-e2e`, `secret-scan` and `commitlint` must pass.
- History is linear and merging is **squash only**. The PR title becomes the commit message on
  `master` (the PR body is not copied), so the title must be a valid Conventional Commit.
  `gh pr merge <n> --squash --delete-branch` does this from the CLI.
- No approval is required while there is a single maintainer. Repository admins can bypass the
  rule on a PR only, never with a direct push. Turn on "Require review from Code Owners" in the
  ruleset when a second person joins (see [`.github/CODEOWNERS`](.github/CODEOWNERS)).

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
