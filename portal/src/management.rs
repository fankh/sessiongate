use std::{collections::BTreeSet, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Approver,
    Auditor,
    Administrator,
}

impl FromStr for Role {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "user" => Ok(Self::User),
            "approver" => Ok(Self::Approver),
            "auditor" => Ok(Self::Auditor),
            "administrator" => Ok(Self::Administrator),
            _ => Err(format!("unknown role: {value}")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Permission {
    ViewAssignedDestination,
    LaunchAssignedSession,
    ApproveSession,
    ViewOwnAudit,
    ViewAllAudit,
    TerminateOwnSession,
    TerminateAnySession,
    ManageUsers,
    ManageDestinations,
    ManagePolicies,
    ManageCredentials,
    ViewSystemHealth,
}

pub fn permits(roles: &BTreeSet<Role>, permission: Permission) -> bool {
    if roles.contains(&Role::Administrator) {
        return true;
    }
    match permission {
        Permission::ViewAssignedDestination | Permission::LaunchAssignedSession => {
            roles.contains(&Role::User) || roles.contains(&Role::Approver)
        }
        Permission::ApproveSession => roles.contains(&Role::Approver),
        Permission::ViewOwnAudit | Permission::TerminateOwnSession => {
            roles.contains(&Role::User) || roles.contains(&Role::Approver)
        }
        Permission::ViewAllAudit => roles.contains(&Role::Auditor),
        Permission::TerminateAnySession
        | Permission::ManageUsers
        | Permission::ManageDestinations
        | Permission::ManagePolicies
        | Permission::ManageCredentials
        | Permission::ViewSystemHealth => false,
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialReferenceInput {
    pub name: String,
    pub provider: CredentialProvider,
    pub external_ref: String,
}

impl CredentialReferenceInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.len() > 128 {
            return Err("credential reference name must contain 1-128 characters".into());
        }
        if self.external_ref.trim().is_empty()
            || self.external_ref.len() > 512
            || self.external_ref.chars().any(char::is_control)
        {
            return Err("external credential reference is invalid".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialProvider {
    LocalEncrypted,
    HcpVaultSecrets,
    AzureKeyVault,
}

#[derive(Clone, Debug, Serialize)]
pub struct CredentialReferenceView {
    pub id: String,
    pub name: String,
    pub provider: CredentialProvider,
    pub external_ref: String,
    pub status: CredentialStatus,
    pub last_rotated_at: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialStatus {
    Unknown,
    Healthy,
    Unavailable,
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roles(values: &[Role]) -> BTreeSet<Role> {
        values.iter().copied().collect()
    }

    #[test]
    fn user_cannot_cross_management_boundary() {
        let user = roles(&[Role::User]);
        assert!(permits(&user, Permission::LaunchAssignedSession));
        assert!(permits(&user, Permission::TerminateOwnSession));
        assert!(!permits(&user, Permission::ManageUsers));
        assert!(!permits(&user, Permission::ViewAllAudit));
    }

    #[test]
    fn auditor_is_read_only() {
        let auditor = roles(&[Role::Auditor]);
        assert!(permits(&auditor, Permission::ViewAllAudit));
        assert!(!permits(&auditor, Permission::LaunchAssignedSession));
        assert!(!permits(&auditor, Permission::TerminateAnySession));
    }

    #[test]
    fn administrator_has_every_permission() {
        let admin = roles(&[Role::Administrator]);
        for permission in [
            Permission::ViewAssignedDestination,
            Permission::ApproveSession,
            Permission::ViewAllAudit,
            Permission::TerminateAnySession,
            Permission::ManageUsers,
            Permission::ManageDestinations,
            Permission::ManagePolicies,
            Permission::ManageCredentials,
            Permission::ViewSystemHealth,
        ] {
            assert!(permits(&admin, permission));
        }
    }

    #[test]
    fn credential_contract_contains_reference_not_secret() {
        let input: CredentialReferenceInput = serde_json::from_value(serde_json::json!({
            "name": "Windows administrators",
            "provider": "local_encrypted",
            "external_ref": "local/windows-admin"
        }))
        .unwrap();
        input.validate().unwrap();
        assert!(
            serde_json::from_value::<CredentialReferenceInput>(serde_json::json!({
                "name": "bad",
                "provider": "local_encrypted",
                "external_ref": "local/bad",
                "password": "must-not-be-accepted"
            }))
            .is_err()
        );
    }
}
