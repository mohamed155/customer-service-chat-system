use config::Environment;
use identity::{keys, session, sessions};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn start(pool: &PgPool, environment: &Environment, user: Uuid) -> sessions::Issued {
    sessions::start(
        pool,
        keys::keys_for(environment),
        user,
        session::SessionAuth::password(),
        &sessions::ClientMeta::default(),
    )
    .await
    .unwrap()
}

/// Each integration-test executable that exercises production mode installs its generated
/// signing key before constructing any router. All tests in that executable share the key.
#[allow(dead_code)] // Only the suites that exercise production-mode cookies call this.
pub fn configure_keys() {
    static CONFIGURED: std::sync::Once = std::sync::Once::new();
    CONFIGURED.call_once(|| {
        let key = keys::generated_key_json("integration-test").unwrap();
        std::env::set_var(keys::KEYS_ENV, key);
    });
}
