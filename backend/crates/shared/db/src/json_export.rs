//! Row-at-a-time database export without buffering a complete dataset.

use futures::TryStreamExt;
use serde_json::Value;
use sqlx::postgres::PgArguments;
use sqlx::query::QueryScalar;
use sqlx::{PgConnection, Postgres};

pub trait JsonExport: Send {
    fn start(&mut self, name: &str) -> std::io::Result<()>;
    fn row(&mut self, value: &Value) -> std::io::Result<()>;
    fn end(&mut self) -> std::io::Result<()>;
}

pub async fn write_rows(
    conn: &mut PgConnection,
    export: &mut dyn JsonExport,
    name: &str,
    query: QueryScalar<'_, Postgres, Value, PgArguments>,
) -> sqlx::Result<()> {
    export.start(name)?;
    let mut rows = query.fetch(conn);
    while let Some(row) = rows.try_next().await? {
        export.row(&row)?;
    }
    export.end()?;
    Ok(())
}
