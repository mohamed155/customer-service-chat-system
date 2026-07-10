use std::{fmt, str::FromStr};

/// Tenant-scoped roles stored on active memberships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TenantRole {
    Owner,
    Admin,
    Manager,
    Agent,
    Viewer,
}

impl fmt::Display for TenantRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Manager => "manager",
            Self::Agent => "agent",
            Self::Viewer => "viewer",
        })
    }
}

impl FromStr for TenantRole {
    type Err = String;

    fn from_str(role: &str) -> Result<Self, Self::Err> {
        match role {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "manager" => Ok(Self::Manager),
            "agent" => Ok(Self::Agent),
            "viewer" => Ok(Self::Viewer),
            _ => Err(format!("invalid tenant role: {role}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TenantRole;

    #[test]
    fn tenant_role_display_fromstr_round_trip() {
        let roles = [
            TenantRole::Owner,
            TenantRole::Admin,
            TenantRole::Manager,
            TenantRole::Agent,
            TenantRole::Viewer,
        ];

        for role in roles {
            assert_eq!(role.to_string().parse::<TenantRole>(), Ok(role));
        }
    }

    #[test]
    fn tenant_role_rejects_invalid_values() {
        assert!("support".parse::<TenantRole>().is_err());
        assert!("OWNER".parse::<TenantRole>().is_err());
        assert!("".parse::<TenantRole>().is_err());
    }
}
