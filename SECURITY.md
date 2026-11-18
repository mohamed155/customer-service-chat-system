# Security Policy

## Reporting a vulnerability

Please report security vulnerabilities **privately**, through GitHub:

1. Go to the repository's **Security** tab and choose **Report a vulnerability**, or open
   <https://github.com/mohamed155/customer-service-chat-system/security/advisories/new> directly.
2. Describe the issue, the affected component or endpoint, how to reproduce it, and the impact you
   expect. A proof of concept helps, as long as it doesn't touch other people's data.

**Don't** report vulnerabilities in public issues, pull requests or discussions. The report stays
private between you and the maintainers until a fix is released.

## What to expect

| Step | Target |
| -- | -- |
| Acknowledgement of your report | Within 3 business days |
| Initial assessment (whether it is a vulnerability, and its severity) | Within 7 days |
| Fix and coordinated disclosure | Within 90 days of the report; sooner for critical issues |

We'll keep you updated while we work on it, agree a disclosure date with you, and credit you in the
published advisory unless you'd rather stay anonymous. If we can't meet a target, we'll tell you
why and when to expect an answer.

## Supported versions

The platform is developed trunk-based and released from `master`. Security fixes go to `master`
and ship with the next release; older builds are not patched separately.

## Scope

In scope: the code in this repository, which includes the backend API, the dashboard, the chat
widget, the infrastructure configuration and the operations scripts. Vulnerabilities in a
third-party dependency are in scope when they affect this project; please report those upstream
too.

When testing, only use accounts and tenants you created yourself. Don't access or change other
users' data, degrade the service for others, or use social engineering.

## Dependency policy

Principle II of the [constitution](.specify/memory/constitution.md) asks for vulnerability **and
licence** scanning in CI. Three checks run on every pull request:

| Check | Where | What it fails on |
| -- | -- | -- |
| `cargo audit` | Backend workflow | A known vulnerability (RustSec) in a Rust dependency |
| `cargo deny check licenses bans sources` | Backend workflow, policy in [`backend/deny.toml`](backend/deny.toml) | A dependency under a licence that is not allowed, a banned crate, or a crate that does not come from crates.io |
| `pnpm audit --prod --audit-level=high` | Frontend workflow | An advisory of high or critical severity in a production dependency of the dashboard or the widget |

### Licences

Allowed: MIT, Apache-2.0 (also with the LLVM exception), BSD-2-Clause, BSD-3-Clause, ISC,
Unicode-3.0, Zlib, 0BSD, MIT-0, CC0-1.0, Unlicense, BSL-1.0 and CDLA-Permissive-2.0, which are
permissive, and MPL-2.0, which is copyleft **per file**: it applies to the crate's own source files
and does not reach this code as long as the crate is not modified (today `cssparser` and
`dtoa-short`, which `ammonia`, the HTML sanitiser of the knowledge module, depends on).

Not allowed until someone decides otherwise: GPL, AGPL, SSPL, LGPL-only licences, a missing
licence, and anything not on the list. A dependency under `A OR B` passes when either is allowed
(`r-efi` offers LGPL-2.1-or-later *or* MIT *or* Apache-2.0, so it passes under MIT). The workspace's
own crates are unpublished (`publish = false`) and are not checked.

To allow a new licence, add it to `allow` in `backend/deny.toml` in the pull request that needs it
and say in the description which crate brings it in, what the licence requires of us, and why that
is acceptable. For the frontend, `pnpm licenses list --prod` shows the licences of the production
dependencies; a licence outside the permissive list needs the same decision. Today every production dependency
is MIT, Apache-2.0, BSD, ISC, 0BSD, BlueOak-1.0.0, Python-2.0 or OFL-1.1 (two font packages).

### Banned crates and sources

`openssl`, `openssl-sys` and `native-tls` are banned: TLS is rustls everywhere, so there is one
set of certificates and one security update path. Dependencies come from crates.io only; a git
dependency or an extra registry fails the check. Two versions of one crate are reported as a
warning in the CI log, not blocked.

### Dependency audit (frontend)

`pnpm audit --prod --audit-level=high` runs in the Frontend workflow. Moderate and low advisories
are listed in the log and do not fail the build; fix them when an update is available.

**Ignoring an advisory** is a last resort, for an advisory with no fix or one that cannot reach us.
Add its GHSA id under `auditConfig.ignoreGhsas` in
[`frontend/pnpm-workspace.yaml`](frontend/pnpm-workspace.yaml) with a comment that says where the
vulnerable package comes from, why it cannot be exploited here, and when to remove the entry. Say
the same in the pull request. A waiver without a reason is not accepted in review.

Current waivers and pins:

| Item | Why | Remove when |
| -- | -- | -- |
| GHSA-fc5q-39xc-vjpf (`picomatch` <= 4.0.7, high) | No patched version exists. Only reached through `@taiga-ui/cdk` > `@angular-devkit/core`, the Angular tooling behind `ng add` and `ng update`. It is not in the browser bundle (a search of the built dashboard finds no `picomatch`), and it only matches file paths of the developer's own project. | `picomatch` > 4.0.7 is published and in the lockfile |
| `minimatch` 10.2.6 (moderate, GHSA-g3hv-75wg-m249) | Below the gate. Only reached through `@taiga-ui/cdk` > `ng-morph` (schematics), and not in the browser bundle. The patched version is a new major (11.x). | `ng-morph` or `@taiga-ui/cdk` moves to `minimatch` 11 |
| `@tiptap/*` pinned to 3.30.5 (`pnpm.overrides` in `frontend/package.json`) | `@tiptap/core` 3.28.0 had two advisories (GHSA-cp6q-959q-f8rh, GHSA-j95f-988m-3j2f), fixed in 3.30.5, and `@taiga-ui/editor` asks for `^3.30.5` anyway. Pinned exactly because 3.34.0 adds about 15 kB to the editor chunk and breaks its 440 kB budget in `angular.json`. | `@taiga-ui/editor` is updated: raise the pins together, with the budget if the chunk grows |

## Security design

How the platform is secured, including the threat model and tenant isolation, is described in
[docs/security/](docs/security/README.md).
