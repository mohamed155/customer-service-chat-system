# Contributing

Thanks for contributing to the AI Customer Service Platform. The rules every change
must follow live in the [project constitution](.specify/memory/constitution.md); this
page covers the day-to-day workflow.

## Workflow

1. **Start from a Linear issue.** Features go through the
   [Spec Kit workflow](#spec-kit-workflow) first: specify → clarify → plan → tasks → implement.
2. **Branch** from `master` using the Linear branch name (see
   [Branch names](#branch-names)). Keep branches short-lived.
3. **Commit** with [Conventional Commits](#commit-messages)
   (`feat(auth): ...`, `fix(db): ...`, `docs: ...`).
4. **Run the checks** before pushing: format, lint, type check, build, and tests for the
   parts you touched. CI must be green before merge.
5. **Open a pull request.** The PR template
   ([`.github/pull_request_template.md`](.github/pull_request_template.md)) loads
   automatically.

## Spec Kit workflow

Every feature is specified before it is built. The steps come from the constitution's
_Development Workflow and Quality Gates_, and each one has a command. Agents run them as
`/speckit.<step>` (OpenCode) or `/speckit-<step>` (Claude Code skills in `.claude/skills/`):

| Step | Command | Output |
| -- | -- | -- |
| 1. Specify | `speckit.specify` | `specs/NNN-feature/spec.md`: problem, scope, non-goals, roles, tenancy and security impact, data, API, UX states, acceptance criteria |
| 2. Clarify | `speckit.clarify` | Open questions answered and recorded in the spec, before planning |
| 3. Plan | `speckit.plan` | `plan.md` with a Constitution Check against every principle; deviations listed under Complexity Tracking |
| 4. Tasks | `speckit.tasks` | `tasks.md`: small, ordered tasks, each independently verifiable and with its tests |
| 5. Implement | `speckit.implement` | The code, one task per cycle, with format, lint, build and tests run each time |

Each feature gets its own folder in [`specs/`](specs/), numbered in order (`033-production-hardening`).
Templates are in `.specify/templates/`.

## Trunk-based workflow

`master` is the trunk. It is always releasable, and everything reaches it through a pull request.

- **One trunk.** No long-lived `develop`, `release/*` or per-environment branches.
- **Short-lived branches.** Branch from the latest `master`, one Linear issue per branch, and
  merge within a day or two. Split work that would take longer into smaller issues and PRs.
- **Small PRs, merged often.** Keep `master` green: a PR merges only when its required checks
  pass (see [Merging](#merging)). Rebase or merge `master` into your branch if it falls behind;
  never force-push `master`.
- **Unfinished work ships dark.** If a feature can't land in one PR, merge it behind a flag or
  without wiring it into the UI or routes, so `master` stays releasable.
- **Releases come from `master`.** Each push to `master` that passes CI builds and publishes
  SHA-tagged images and a release manifest (the `release` job in
  [`backend.yml`](.github/workflows/backend.yml)). Fix forward with a new PR, or revert the
  offending squash commit.

## Branch names

Use the branch name Linear generates for the issue (the issue's **Copy git branch name** action),
for example `mohamedahmedramadan154/moh-20-commit-conventions-commitlint`. It contains the
issue ID (`moh-20`), which is what links the branch and its pull request to the Linear issue
automatically. Don't rename it. Work that has no Linear issue doesn't start until one exists.

## Commit messages

Commits **and pull request titles** follow [Conventional Commits](https://www.conventionalcommits.org/):

```text
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
- **In CI:** the `commitlint` job in [`.github/workflows/ci.yml`](.github/workflows/ci.yml) checks the PR
  title and every commit in the PR.
- **Docs and Compose:** the `docs-lint` job runs markdownlint on `docs/` and the root markdown files
  (locally: `npx markdownlint-cli2@0.24.0`). The `docs-links` job fails on a broken internal link or
  heading anchor; `docs-links-external` only warns about dead external links. The `compose-config` job
  runs `docker compose config` for every profile and the staging overlay.

### Dependencies and secrets

- **Dependabot** ([`.github/dependabot.yml`](.github/dependabot.yml)) opens pull requests every Monday for
  GitHub Actions, the frontend (npm/pnpm) and the backend (Cargo). Minor and patch updates are grouped
  into one pull request per ecosystem; major updates come one by one. Titles use `ci(deps)` or
  `build(deps)` so they pass commitlint.
- **Workflow supply chain:** every `uses:` in `.github/workflows/` is pinned to a full commit SHA with
  the tag in a trailing comment (`uses: actions/checkout@<sha> # v7.0.1`), and every workflow declares
  top-level `permissions:` (default `contents: read`; a job that needs more declares it itself). The
  `workflow-pinning` job fails on a tag-pinned action. Dependabot bumps the SHA and the comment together.
  To add an action, look up the commit of the release tag
  (`gh api repos/<owner>/<repo>/commits/<tag> --jq .sha`), and check locally with
  `node .github/scripts/check-workflows.mjs`.
- **Secret scanning:** the `secret-scan` job runs gitleaks over the full git history and fails the build
  on any finding. Rules and the list of known fake values are in [`.gitleaks.toml`](.gitleaks.toml). If a
  test fixture trips it, add that exact value to the allowlist there, in the same pull request.

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

Mark an item `N/A` with a one-line reason if it does not apply. The constitution asks for at
least one approval, and every reviewer confirms constitution compliance before approving. While
there is a single maintainer the ruleset does not enforce approvals (see [Merging](#merging)).

## Definition of Done

An issue is Done only when all of these hold (from the constitution):

- Acceptance criteria verified.
- Tests added and passing; coverage of new domain logic at least 80% of lines.
- Tenant-isolation and authorization tests present where tenant data or permissions are touched.
- No new lint warnings and no unresolved review comments.
- OpenAPI, ADRs, module READMEs and user docs updated to match the code.
- Observability (logs, traces, metrics) in place for new flows.
- Migrations reversible, or explicitly documented as forward-only.

The workflow's last step adds one more: acceptance criteria are verified in the QA environment
before the issue is Done.

## Where to read more

- [Constitution](.specify/memory/constitution.md): principles, technology standards,
  Definition of Done
- [`frontend/CLAUDE.md`](frontend/CLAUDE.md): frontend conventions
- [`README.md`](README.md): local setup and quickstart
- [`docs/`](docs/README.md): architecture, ADRs, specs index, runbooks (deployment, backup/restore, operations), security, configuration
- [SECURITY.md](SECURITY.md): reporting a vulnerability
