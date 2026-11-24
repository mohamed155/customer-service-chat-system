use sqlx::PgConnection;
use uuid::Uuid;

pub struct NotificationPort;

#[async_trait::async_trait]
impl privacy::Notifications for NotificationPort {
    async fn export_customer(
        &self,
        conn: &mut PgConnection,
        tenant: Uuid,
        conversations: &[Uuid],
        export: &mut dyn db::json_export::JsonExport,
    ) -> sqlx::Result<()> {
        let escalations =
            escalations::customer_privacy::subject_ids(conn, tenant, conversations).await?;
        let tools = tools::customer_privacy::subject_ids(conn, tenant, conversations).await?;
        notifications::customer_privacy::export_customer(
            conn,
            tenant,
            conversations,
            &escalations,
            &tools,
            export,
        )
        .await
    }

    async fn erase_customer(
        &self,
        conn: &mut PgConnection,
        tenant: Uuid,
        conversations: &[Uuid],
    ) -> sqlx::Result<()> {
        let escalations =
            escalations::customer_privacy::subject_ids(conn, tenant, conversations).await?;
        let tools = tools::customer_privacy::subject_ids(conn, tenant, conversations).await?;
        notifications::customer_privacy::erase_customer(
            conn,
            tenant,
            conversations,
            &escalations,
            &tools,
        )
        .await
    }
}
