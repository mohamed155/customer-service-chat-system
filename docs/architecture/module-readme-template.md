# Module README template

Every backend module (`backend/crates/modules/<name>/`) has a `README.md` that tells a new
engineer, or an agent, what the module is for and how to use it without reading its code. The
constitution requires it (Principle IX: responsibilities, public interface, owned data, emitted
events).

Copy the template below into `backend/crates/modules/<name>/README.md`, replace every
`<placeholder>`, and delete the guidance in italics. Write `None` in a section that doesn't apply
instead of deleting it, so readers know it was considered. Update the README in the same pull
request that changes what it describes.

## Template

````markdown
# `<name>` module

<One sentence: what this module is responsible for, in product terms.>

## Responsibility

_What this module owns and decides, and what it deliberately does not do (name the module that
does)._

- Owns: <capabilities>
- Does not: <capability> (see `<other module>`)

## Public interface

_Everything other code may use. Anything not listed here is internal._

### HTTP endpoints

| Method | Path | Permission | Purpose |
| -- | -- | -- | -- |
| `GET` | `/api/v1/tenant/<resource>` | `<resource>.view` | <purpose> |

_Paths are relative to the server root. The OpenAPI document is the full contract; link its
section if this list gets long._

### Rust API for other modules

| Item | Purpose |
| -- | -- |
| `<name>::<function or type>` | <what callers use it for> |

## Owned tables

_Only this module reads or writes these tables. Other modules go through the public interface or
events (Principle IV)._

| Table | Tenant-scoped | Purpose |
| -- | -- | -- |
| `<table>` | Yes (`tenant_id`) | <what a row is> |

Migrations: <list the files in `backend/migrations/` that create or change these tables>.

## Events

### Published

| Event type | When | Payload |
| -- | -- | -- |
| `<aggregate>.<event>` | <trigger> | <key fields> |

_Written to `outbox_events` in the same transaction as the change that caused them._

### Consumed

| Event type | From | What this module does |
| -- | -- | -- |
| `<aggregate>.<event>` | `<module>` | <reaction> |

## Permissions

_The permissions this module defines or checks (`authz::Permission`), and who has them by default._

| Permission | Allows | Default roles |
| -- | -- | -- |
| `<resource>.view` | <action> | Owner, Admin |

Audit: <the actions that write an audit record, e.g. `<aggregate>.<action>`>.

## Configuration

_Environment variables this module reads. Link to [docs/configuration.md](../../../../docs/configuration.md)
rather than repeating defaults._

## Tests

_Where the tests live and anything needed to run them (e.g. `REQUIRE_DB_TESTS=1` with the local
services up), including the cross-tenant isolation tests._
````

## Notes

- **Relative links**: a README in `backend/crates/modules/<name>/` is four levels below the
  repository root, so links to `docs/` start with `../../../../`.
- **Event names**: use the `event_type` string exactly as it is written to the outbox or audit log,
  so readers can search for it.
- **Permissions**: use the serialized name (`conversations.manage`), which is what the API and the
  dashboard see, not the Rust variant name.
