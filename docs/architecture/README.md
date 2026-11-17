# Architecture

How the platform is built and why.

| Document | What it covers |
| -- | -- |
| [System architecture](system-architecture.md) | Quality priorities, architectural style, the module map, code organization, tenancy, security, data, realtime, AI runtime, scalability and reliability |
| [Design tokens](design-tokens.md) | The dashboard's color rule, the token to use for each job, how to add one, and the `pnpm lint:colors` check |
| [Accessibility checks](accessibility.md) | The axe-core, reduced-motion and theme-state e2e checks, the known-violation list, and the rules they led to |
| [Right-to-left layout](right-to-left.md) | How the language sets the direction, how to write a style that mirrors, directional icons and arrows, the widget, and the `pnpm lint:rtl` check |
| [Module README template](module-readme-template.md) | What every backend module's `README.md` states: responsibility, public interface, owned tables, events, permissions |

Decisions behind the architecture are recorded in the [ADRs](../adr/README.md). The
[constitution](../../.specify/memory/constitution.md) sets the rules the architecture has to meet.
