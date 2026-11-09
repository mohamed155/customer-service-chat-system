# Security

Security design notes for the platform. To report a vulnerability, follow
[SECURITY.md](../../SECURITY.md); don't open a public issue.

| Document | What it covers |
| -- | -- |
| [Threat model](threat-model.md) | Assets, trust boundaries, owners, STRIDE threats per module and the top risks |
| [Tenant Unit of Work](tenant-unit-of-work.md) | How backend code reaches tenant data, and which queries deliberately don't |

The security rules every change follows are in the
[constitution](../../.specify/memory/constitution.md) (Principles I to III). Security decisions are
recorded as ADRs, among them [multi-tenancy](../adr/0004-multi-tenancy-rls-cells.md),
[authentication](../adr/0005-authentication-tokens.md),
[authorization](../adr/0006-authorization-model.md) and
[tenant secrets](../adr/0014-envelope-encryption-tenant-secrets.md).
