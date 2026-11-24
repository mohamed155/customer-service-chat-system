# Customer export and erasure requests

Act in the customer's tenant through the tenant API using an authorized Owner or Admin
account. Complete MFA step-up when prompted. A standalone worker
(`APP_RUN_MODE=worker`) requires shared S3-compatible storage and exits if its selected
backend is in memory.

1. Send `POST /api/v1/tenant/customers/{id}/export` or
   `POST /api/v1/tenant/customers/{id}/erase` with a JSON body containing
   `{"confirm":"<customer id>"}`.
2. Keep the returned `request_id` and query
   `GET /api/v1/tenant/customers/{id}/privacy-requests/{request_id}`.
3. For a completed export, retrieve
   `GET /api/v1/tenant/customers/{id}/export/{request_id}/download` before expiry.
   Treat the ZIP as personal data and deliver it through the approved secure channel.

Erasure starts with an erasure-pending customer and completes after the worker finishes all
steps. A repeat POST returns the same receipt. The worker retries transient failures with a
delay and skips completed steps. For a persistent failure, inspect worker health, verify SQL,
cache and object-storage availability, and review generic failure codes in the request record.
After resolving the underlying fault, an operator can requeue an exhausted erase request in
a tenant-scoped transaction. Bind the validated tenant ID and request ID as `$1` and `$2`.
Keep the existing request, its completed steps, and object locators.

```sql
UPDATE privacy_requests
   SET status = 'queued', attempts = 0, available_at = now(), failure_code = NULL
 WHERE tenant_id = $1 AND id = $2
   AND kind = 'erase' AND status = 'failed' AND lease_token IS NULL;
```

Confirm exactly one row changed and poll the existing request ID.

For restored backups, [replay post-backup erasures](backup-restore.md#replay-customer-erasures)
and [reconcile restored privacy objects](backup-restore.md#reconcile-restored-privacy-objects)
before serving data. Repeating an erase POST returns the existing receipt. Restored objects
whose locators are already gone require the separate reconciliation step.
Consult the [inventory](../security/personal-data-inventory.md) for retention exceptions.
