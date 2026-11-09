# Specifications

Feature specifications live in the top-level [`specs/`](../../specs/) folder, not here: the Spec Kit
tooling reads and writes them there. This page explains how to find your way around them.

## Layout

Each feature has one numbered folder, in the order the features were specified, for example
[`specs/033-production-hardening/`](../../specs/033-production-hardening/). A folder usually holds:

| File | What it is |
| -- | -- |
| `spec.md` | The problem, scope, non-goals, roles, tenancy and security impact, and acceptance criteria |
| `plan.md` | How it is built, with the Constitution Check |
| `tasks.md` | The ordered, verifiable tasks |
| `research.md`, `data-model.md`, `contracts/`, `quickstart.md` | Supporting design notes, written when the plan needs them |

Specs are records of what was decided when the feature was built. Later changes go in a new
spec, not in an old one, so file paths inside an old spec can be out of date.

## Start here

- [`specs/001-ai-customer-service-platform/spec.md`](../../specs/001-ai-customer-service-platform/spec.md):
  the platform-wide software requirements specification.
- [`specs.md`](../../specs.md): the roadmap that orders the features.
- [CONTRIBUTING.md → Spec Kit workflow](../../CONTRIBUTING.md#spec-kit-workflow): how to write
  a new spec.
