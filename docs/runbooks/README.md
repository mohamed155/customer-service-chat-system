# Runbooks

Step-by-step procedures for operating the platform in staging and production. The scripts they
call are in [`ops/`](../../ops/).

| Runbook | When to use it |
| -- | -- |
| [Deployment](deployment.md) | Bootstrap a fresh host, promote a release, roll back, and the limits of rollback after destructive migrations |
| [Backup and restore](backup-restore.md) | The backup schedule, what is captured, RPO and RTO, restoring, and the restore drill |
| [Operations](operations.md) | Five everyday tasks: promote a release, roll back, restore a backup, rotate a credential, investigate a reported error |

Every environment variable these procedures mention is described in the
[configuration reference](../configuration.md). For local development, see the
[README quickstart](../../README.md#quickstart) instead.
