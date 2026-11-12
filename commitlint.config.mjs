// Conventional Commits for commits and PR titles (see CONTRIBUTING.md).
// Run from the repo root: `pnpm --dir frontend exec commitlint --config ../commitlint.config.mjs`
// Installed with `pnpm install` in frontend/ (commitlint + lefthook are devDependencies there).

// Scopes mirror the backend modules and shared crates, the frontend apps, and the cross-cutting
// areas this repo already uses. Add a scope here in the same PR that introduces the module.
const scopes = [
  // backend modules (backend/crates/modules/*)
  "ai",
  "analytics",
  "audit",
  "authz",
  "billing",
  "conversations",
  "customers",
  "email",
  "escalations",
  "feedback",
  "flags",
  "identity",
  "integrations",
  "knowledge",
  "notifications",
  "platform",
  "prompts",
  "rbac",
  "secrets",
  "telegram",
  "tenancy",
  "tools",
  "users",
  "whatsapp",
  "widgets",
  // backend shared crates and server (backend/crates/shared/*, ai-providers, server)
  "ai-providers",
  "server",
  "cache",
  "config",
  "db",
  "events",
  "kernel",
  "observability",
  "storage",
  // frontend
  "frontend",
  "backend",
  "web",
  "dashboard",
  "widget",
  // cross-cutting
  "api",
  "auth",
  "rate-limit",
  "adr",
  "docs",
  "infra",
  "ops",
  "loadtest",
  "ci",
  "deps",
  "security",
  "specs",
  "claude",
  "release",
];

export default {
  extends: ["@commitlint/config-conventional"],
  // Merge commits ("Merge pull request #12 ...", "Merge branch ...") are created by GitHub/git.
  defaultIgnores: true,
  rules: {
    "scope-enum": [2, "always", scopes],
    // Subjects often start with an acronym or identifier (ADR-012, API, UUIDv7).
    "subject-case": [0],
    // Commit bodies include URLs, tables and attribution lines.
    "body-max-line-length": [0],
    "footer-max-line-length": [0],
    "header-max-length": [2, "always", 100],
  },
};
