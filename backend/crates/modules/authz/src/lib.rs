//! Authorization module - permission vocabulary, role matrices, and request guards.
//!
//! # Purpose
//! Provide the canonical authorization policy used by backend route enforcement.
//!
//! # Responsibilities
//! - Define tenant and platform permission codes.
//! - Map stored roles to static permission sets.
//! - Enforce required permissions at Axum route boundaries.
//!
//! # Public Interfaces
//! - [`Permission`] and [`TenantRole`] policy vocabulary.
//! - [`tenant_role_permissions`], [`platform_role_permissions`], and
//!   [`staff_tenant_permissions`] role matrices.
//! - [`PermissionSet`] request extension and [`require_permission`] route layer.
//!
//! # Dependencies
//! Depends on `identity` for platform principals, `kernel` for API errors, and
//! Axum for request enforcement. It does not access persistence directly.
//!
//! # Data Model
//! Permissions and role mappings are release-static enums and slices. No
//! authorization state is persisted by this module.
//!
//! # Extension Points
//! Add permission variants and update every exhaustive role matrix when the
//! authorization contract changes. Add guards here rather than in handlers.

pub mod guard;
pub mod matrix;
pub mod permission;
pub mod role;

pub use guard::{PermissionSet, platform_permission_middleware, require_permission};
pub use matrix::{platform_role_permissions, staff_tenant_permissions, tenant_role_permissions};
pub use permission::Permission;
pub use role::TenantRole;
