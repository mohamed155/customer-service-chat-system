//! Unit of Work: a transaction that carries the tenant for Row-Level Security.
//!
//! # Purpose
//! ADR-004 §2: every tenant transaction sets `app.tenant_id` **transaction-locally** before it
//! touches tenant data, on the same connection that runs the queries. A [`UnitOfWork`] does both
//! steps in one place: it opens the transaction and runs
//! `SELECT set_config('app.tenant_id', $1, true)`. The third argument `true` makes the setting
//! local to the transaction, so it is gone at commit or rollback and can never leak to the next
//! user of a pooled connection (it is also safe behind PgBouncer in transaction mode). A
//! session-level `SET` is forbidden.
//!
//! # Public Interfaces
//! - [`TenantScoped`]: implemented by the authorised tenant context (`tenancy::TenantContext`)
//! - [`UnitOfWork::begin`]: open a tenant transaction from a [`TenantScoped`] value
//! - [`UnitOfWork::begin_for_tenant`]: open one from a bare tenant id, for code that has no request
//!   context (background jobs that already resolved the tenant, ADR-007)
//!
//! # Extension Points
//! Repositories take `&mut PgConnection` (through `&mut *uow`) or `&mut Transaction` (through
//! `&mut uow`), so a repository never opens its own pool connection for tenant data.

use sqlx::{PgPool, Postgres, Transaction};
use std::ops::{Deref, DerefMut};
use uuid::Uuid;

/// Something that names exactly one tenant and was produced by the authorisation layer.
pub trait TenantScoped {
    fn tenant_id(&self) -> Uuid;
}

impl TenantScoped for Uuid {
    fn tenant_id(&self) -> Uuid {
        *self
    }
}

/// A database transaction whose `app.tenant_id` is set for its whole lifetime.
///
/// Dropping it without calling [`commit`](UnitOfWork::commit) rolls the transaction back.
pub struct UnitOfWork {
    tx: Transaction<'static, Postgres>,
    tenant_id: Uuid,
}

impl UnitOfWork {
    /// Opens a transaction for the tenant of `scope` and sets `app.tenant_id` on its connection.
    pub async fn begin(pool: &PgPool, scope: &impl TenantScoped) -> sqlx::Result<Self> {
        Self::begin_for_tenant(pool, scope.tenant_id()).await
    }

    /// Opens a transaction for `tenant_id`. Prefer [`begin`](UnitOfWork::begin) wherever a request
    /// context exists; use this only where the tenant was already resolved from a trusted source
    /// (a job payload, an authenticated session row).
    pub async fn begin_for_tenant(pool: &PgPool, tenant_id: Uuid) -> sqlx::Result<Self> {
        let mut tx = pool.begin().await?;
        sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
            .bind(tenant_id.to_string())
            .execute(&mut *tx)
            .await?;
        Ok(Self { tx, tenant_id })
    }

    /// The tenant this transaction is bound to.
    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    /// The connection that runs this transaction, for `.execute(uow.conn())` and friends. Code that
    /// takes a `&mut Transaction` (the `*_in_tx` helpers) can be given `&mut uow` directly.
    pub fn conn(&mut self) -> &mut sqlx::PgConnection {
        &mut self.tx
    }

    pub async fn commit(self) -> sqlx::Result<()> {
        self.tx.commit().await
    }

    pub async fn rollback(self) -> sqlx::Result<()> {
        self.tx.rollback().await
    }
}

impl Deref for UnitOfWork {
    type Target = Transaction<'static, Postgres>;

    fn deref(&self) -> &Self::Target {
        &self.tx
    }
}

impl DerefMut for UnitOfWork {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tx
    }
}
