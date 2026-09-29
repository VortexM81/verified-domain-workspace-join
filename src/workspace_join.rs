use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::infrai_client::{InfraiClient, InfraiError, UserData};

#[derive(Debug, Clone, Deserialize)]
pub struct JoinRequest {
    pub email: String,
    pub name: String,
    pub company_domain: String,
    pub workspace: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum JoinDecision {
    Join,
    EmailDomainMismatch,
    DomainUnverified,
}

#[derive(Debug, Serialize)]
pub struct JoinResult {
    pub event: &'static str,
    pub user_id: String,
    pub workspace: String,
    pub release_operation: &'static str,
    pub diagnostic: String,
}

#[derive(Debug, Error)]
pub enum JoinError {
    #[error("email domain does not match the requested company domain")]
    EmailDomainMismatch,
    #[error("company domain is not verified")]
    DomainUnverified,
    #[error(transparent)]
    Infrai(#[from] InfraiError),
}

pub fn decide_join(email: &str, company_domain: &str, verified: bool) -> JoinDecision {
    let email_domain = email.rsplit_once('@').map(|(_, domain)| domain);
    if email_domain != Some(company_domain) {
        JoinDecision::EmailDomainMismatch
    } else if !verified {
        JoinDecision::DomainUnverified
    } else {
        JoinDecision::Join
    }
}

pub async fn join_workspace(
    client: &InfraiClient,
    request: &JoinRequest,
) -> Result<JoinResult, JoinError> {
    let domain = client.verify_domain(&request.company_domain).await?;
    match decide_join(&request.email, &request.company_domain, domain.verified) {
        JoinDecision::EmailDomainMismatch => return Err(JoinError::EmailDomainMismatch),
        JoinDecision::DomainUnverified => return Err(JoinError::DomainUnverified),
        JoinDecision::Join => {}
    }

    let user = match client.user_by_email(&request.email).await {
        Ok(user) => user,
        Err(InfraiError::Rejected { status: 404, .. }) => {
            let key = format!("workspace:{}:{}", request.workspace, request.email);
            client
                .create_user(&request.email, &request.name, &request.workspace, &key)
                .await?
        }
        Err(error) => return Err(error.into()),
    };
    Ok(joined(user, request))
}

fn joined(user: UserData, request: &JoinRequest) -> JoinResult {
    JoinResult {
        event: "developer.workspace_joined",
        user_id: user.id,
        workspace: request.workspace.clone(),
        release_operation: "grant_release_access",
        diagnostic: format!(
            "{} matched verified domain {}",
            user.email, request.company_domain
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{decide_join, JoinDecision};

    #[test]
    fn joins_only_a_matching_employee_of_a_verified_domain() {
        assert_eq!(
            decide_join("dev@compiler.example", "compiler.example", true),
            JoinDecision::Join
        );
        assert_eq!(
            decide_join("dev@personal.example", "compiler.example", true),
            JoinDecision::EmailDomainMismatch
        );
        assert_eq!(
            decide_join("dev@compiler.example", "compiler.example", false),
            JoinDecision::DomainUnverified
        );
    }
}
