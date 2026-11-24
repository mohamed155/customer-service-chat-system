# Customer privacy

## Purpose

Export and erase the operational data attributed to one tenant customer.

## Interfaces

The tenant API queues requests and reports progress. Download requires the export permission
and an unexpired completed request. Each POST requires customer ID confirmation and the existing
MFA step-up guard. Owner and Admin receive both permissions by default.

## Ownership and dependencies

Privacy owns `privacy_requests` and `privacy_request_steps`. Owner modules supply their own
export and scrub operations. The server supplies the notifications port. The worker uses a
tenant Unit of Work for each request or step and an audited cross-tenant claim function.

## Worker lifecycle

Workers claim requests with a timed lease. Erasure hides the old contact identity, invalidates
sessions, scrubs content, removes attachments and earlier exports, then finalizes the tombstone.
Each completed step commits separately and can be skipped on retry. The export sweep removes
expired archives and retains request metadata.

See [ADR-020](../../../../docs/adr/0020-customer-data-export-and-erasure.md) and the
[personal data inventory](../../../../docs/security/personal-data-inventory.md).
