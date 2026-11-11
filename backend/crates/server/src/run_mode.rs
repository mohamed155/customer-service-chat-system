//! Run modes: which parts of the server one process runs (ADR-010, ADR-001).
//!
//! The same binary and the same composition root serve every mode; only the set of routes and
//! background workers differs.
//!
//! | Mode | HTTP routes | Background workers |
//! | -- | -- | -- |
//! | `api` | `/api/v1` dashboard, platform and `/auth` routes, API docs | none |
//! | `gateway` | `/api/v1/widget/v1/*`, `/api/v1/hooks/v1/*`, `/integrations/whatsapp/webhook/*` | none |
//! | `worker` | none | all of them |
//! | `all` | everything above | all of them (the default, for local development and e2e) |
//!
//! Every mode also serves `/health`, `/ready` and `/metrics`, so an orchestrator can probe a
//! worker process the same way it probes the others.

use std::str::FromStr;

/// The environment variable that selects the mode. Unset means [`RunMode::All`].
pub const RUN_MODE_ENV: &str = "APP_RUN_MODE";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunMode {
    Api,
    Gateway,
    Worker,
    #[default]
    All,
}

impl RunMode {
    /// Mount the dashboard, platform and `/auth` routes (cookie sessions, CSRF, API docs).
    pub fn serves_api(self) -> bool {
        matches!(self, Self::Api | Self::All)
    }

    /// Mount the public widget, integration-hook and channel-webhook routes.
    pub fn serves_gateway(self) -> bool {
        matches!(self, Self::Gateway | Self::All)
    }

    /// Run the outbox workers and sweepers in this process.
    pub fn runs_workers(self) -> bool {
        matches!(self, Self::Worker | Self::All)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Gateway => "gateway",
            Self::Worker => "worker",
            Self::All => "all",
        }
    }

    /// Read [`RUN_MODE_ENV`]. Unset means [`RunMode::All`]. Anything else must be a mode: a blank
    /// value, a typo or a non-Unicode value is an error, never a silent fallback to `all`, so a
    /// bad production manifest can't start a process that serves everything.
    pub fn from_env() -> Result<Self, String> {
        Self::from_env_value(std::env::var(RUN_MODE_ENV))
    }

    fn from_env_value(value: Result<String, std::env::VarError>) -> Result<Self, String> {
        match value {
            Ok(value) => value.parse(),
            Err(std::env::VarError::NotPresent) => Ok(Self::default()),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(format!("invalid {RUN_MODE_ENV} value: expected Unicode"))
            }
        }
    }
}

impl FromStr for RunMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "api" => Ok(Self::Api),
            "gateway" => Ok(Self::Gateway),
            "worker" => Ok(Self::Worker),
            "all" => Ok(Self::All),
            other => Err(format!(
                "invalid {RUN_MODE_ENV} value '{other}': expected one of api, gateway, worker, all"
            )),
        }
    }
}

impl std::fmt::Display for RunMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_mode_and_rejects_anything_else() {
        for mode in [
            RunMode::Api,
            RunMode::Gateway,
            RunMode::Worker,
            RunMode::All,
        ] {
            assert_eq!(mode.as_str().parse::<RunMode>().unwrap(), mode);
        }
        assert_eq!(" gateway ".parse::<RunMode>().unwrap(), RunMode::Gateway);
        for bad in ["", "API", "gatway", "both", "scheduler"] {
            let error = bad.parse::<RunMode>().unwrap_err();
            assert!(
                error.contains("expected one of api, gateway, worker, all"),
                "{error}"
            );
        }
    }

    #[test]
    fn default_is_all() {
        assert_eq!(RunMode::default(), RunMode::All);
    }

    #[test]
    fn unset_is_all_but_blank_and_non_unicode_are_errors() {
        use std::env::VarError;
        use std::ffi::OsString;

        assert_eq!(
            RunMode::from_env_value(Err(VarError::NotPresent)).unwrap(),
            RunMode::All
        );
        assert!(RunMode::from_env_value(Ok(String::new())).is_err());
        assert!(RunMode::from_env_value(Ok("  ".into())).is_err());
        let error =
            RunMode::from_env_value(Err(VarError::NotUnicode(OsString::from("x")))).unwrap_err();
        assert!(error.contains("Unicode"), "{error}");
        assert_eq!(
            RunMode::from_env_value(Ok("worker".into())).unwrap(),
            RunMode::Worker
        );
    }

    #[test]
    fn each_mode_serves_exactly_what_the_table_says() {
        let table = [
            (RunMode::Api, true, false, false),
            (RunMode::Gateway, false, true, false),
            (RunMode::Worker, false, false, true),
            (RunMode::All, true, true, true),
        ];
        for (mode, api, gateway, workers) in table {
            assert_eq!(mode.serves_api(), api, "{mode} api");
            assert_eq!(mode.serves_gateway(), gateway, "{mode} gateway");
            assert_eq!(mode.runs_workers(), workers, "{mode} workers");
        }
    }

    /// The public side and the internal side must never be served by the same non-`all` mode.
    #[test]
    fn api_and_gateway_are_never_both_served_except_by_all() {
        for mode in [RunMode::Api, RunMode::Gateway, RunMode::Worker] {
            assert!(!(mode.serves_api() && mode.serves_gateway()), "{mode}");
        }
    }
}
