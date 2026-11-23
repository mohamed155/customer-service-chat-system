use config::Environment;
use identity::{keys, session, sessions};
use sqlx::PgPool;
use uuid::Uuid;

#[allow(dead_code)] // Only cookie-authenticated test requests need CSRF proof.
pub const DASHBOARD_ORIGIN: &str = "http://localhost:4200";

pub async fn start(pool: &PgPool, environment: &Environment, user: Uuid) -> sessions::Issued {
    // The configured key set is read once per process. Install the shared key before the first
    // lookup in any environment, or a test that signs in under `Test` first would leave
    // `Production` with no key.
    configure_keys();
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

/// Sign in once and reuse that authentication session across a platform switch and tenant requests.
#[allow(dead_code)] // Only suites that exercise platform access use this helper.
pub struct PlatformAccess {
    cookie: axum::http::HeaderValue,
}

#[allow(dead_code)]
impl PlatformAccess {
    pub async fn sign_in(pool: &PgPool, environment: &Environment, user: Uuid) -> Self {
        if *environment == Environment::Production {
            configure_keys();
        }
        let issued = start(pool, environment, user).await;
        Self {
            cookie: format!("__Host-app_session={}", issued.access_token)
                .parse()
                .unwrap(),
        }
    }

    pub fn request(
        &self,
        mut request: axum::http::Request<axum::body::Body>,
    ) -> axum::http::Request<axum::body::Body> {
        use axum::http::{header, HeaderValue};
        request.headers_mut().remove("x-dev-user-id");
        request
            .headers_mut()
            .insert(header::COOKIE, self.cookie.clone());
        request
            .headers_mut()
            .insert(header::ORIGIN, HeaderValue::from_static(DASHBOARD_ORIGIN));
        request.headers_mut().insert(
            "x-requested-with",
            HeaderValue::from_static("XMLHttpRequest"),
        );
        request
    }

    pub fn scoped_request(
        &self,
        method: axum::http::Method,
        path: &str,
        tenant: Uuid,
        payload: Option<serde_json::Value>,
    ) -> axum::http::Request<axum::body::Body> {
        use axum::{body::Body, http::Request};
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("x-tenant-id", tenant.to_string());
        let request = match payload {
            Some(payload) => request
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string())),
            None => request.body(Body::empty()),
        };
        self.request(request.unwrap())
    }

    pub fn switch(&self, tenant: Uuid) -> axum::http::Request<axum::body::Body> {
        self.request(
            axum::http::Request::post(format!("/api/v1/platform/tenants/{tenant}/switch"))
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"reason":"Run the platform access integration test"}"#,
                ))
                .unwrap(),
        )
    }

    pub async fn open<F, Fut>(
        pool: &PgPool,
        environment: &Environment,
        user: Uuid,
        tenant: Uuid,
        send: F,
    ) -> Self
    where
        F: FnOnce(axum::http::Request<axum::body::Body>) -> Fut,
        Fut: std::future::Future<Output = axum::response::Response>,
    {
        let access = Self::sign_in(pool, environment, user).await;
        let response = send(access.switch(tenant)).await;
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        access
    }
}
