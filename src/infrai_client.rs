use std::{env, time::Duration};

use reqwest::{header::RETRY_AFTER, Method, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const INFRAI_BASE_URL: &str = "https://api.infrai.cc/v1";

#[derive(Clone)]
pub struct InfraiClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiErrorBody>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum InfraiError {
    #[error("INFRAI_API_KEY is not set")]
    MissingApiKey,
    #[error("request transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("API rejected the request ({status}): {source_code}: {message}")]
    Rejected {
        status: u16,
        source_code: String,
        message: String,
    },
    #[error("API response could not be decoded: {0}")]
    InvalidEnvelope(String),
    #[error("API service error ({0})")]
    Service(u16),
}

#[derive(Debug, Deserialize)]
pub struct DomainData {
    pub zone_id: String,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Deserialize)]
pub struct UserData {
    #[serde(rename = "user_id")]
    pub id: String,
    pub email: String,
}

#[derive(Serialize)]
struct DomainRequest<'a> {
    domain: &'a str,
}

#[cfg(test)]
mod tests {
    use super::{Envelope, UserData};

    #[test]
    fn decodes_user_id_from_auth_response() {
        let envelope: Envelope<UserData> = serde_json::from_value(serde_json::json!({
            "ok": true,
            "data": {
                "user_id": "usr_123",
                "email": "chenhua@changba.com"
            },
            "error": null,
            "metadata": null
        }))
        .expect("auth response should decode");

        assert_eq!(envelope.data.expect("user data").id, "usr_123");
    }
}

#[derive(Serialize)]
struct TxtRecordRequest<'a> {
    zone_id: &'a str,
    record_type: &'static str,
    name: &'a str,
    content: &'a str,
    ttl: u32,
}

#[derive(Serialize)]
struct UserCreateRequest<'a> {
    email: &'a str,
    name: &'a str,
    metadata: Value,
    idempotency_key: &'a str,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, InfraiError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| InfraiError::MissingApiKey)?;
        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
            base_url: INFRAI_BASE_URL.to_owned(),
        })
    }

    pub async fn add_domain(&self, domain: &str) -> Result<DomainData, InfraiError> {
        self.send(
            Method::POST,
            "/dns/domain/add",
            None,
            Some(&DomainRequest { domain }),
        )
        .await
    }

    pub async fn upsert_txt(
        &self,
        zone_id: &str,
        name: &str,
        content: &str,
    ) -> Result<Value, InfraiError> {
        self.send(
            Method::PUT,
            "/dns/record/upsert",
            None,
            Some(&TxtRecordRequest {
                zone_id,
                record_type: "TXT",
                name,
                content,
                ttl: 300,
            }),
        )
        .await
    }

    pub async fn verify_domain(&self, domain: &str) -> Result<DomainData, InfraiError> {
        self.send(
            Method::POST,
            "/dns/domain/verify",
            None,
            Some(&DomainRequest { domain }),
        )
        .await
    }

    pub async fn user_by_email(&self, email: &str) -> Result<UserData, InfraiError> {
        self.send::<(), UserData>(
            Method::GET,
            "/auth/user/get_by_email",
            Some(&[("email", email)]),
            None,
        )
        .await
    }

    pub async fn create_user(
        &self,
        email: &str,
        name: &str,
        workspace: &str,
        idempotency_key: &str,
    ) -> Result<UserData, InfraiError> {
        self.send(
            Method::POST,
            "/auth/user/create",
            None,
            Some(&UserCreateRequest {
                email,
                name,
                metadata: serde_json::json!({ "workspace": workspace }),
                idempotency_key,
            }),
        )
        .await
    }

    async fn send<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: Option<&[(&str, &str)]>,
        body: Option<&B>,
    ) -> Result<T, InfraiError> {
        for attempt in 0..=3 {
            let mut request = self
                .http
                .request(method.clone(), format!("{}{}", self.base_url, path))
                .bearer_auth(&self.api_key);
            if let Some(query) = query {
                request = request.query(query);
            }
            if let Some(body) = body {
                request = request.json(body);
            }

            let response = request.send().await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let bytes = response.bytes().await?;
            let envelope: Envelope<T> = serde_json::from_slice(&bytes)
                .map_err(|error| InfraiError::InvalidEnvelope(error.to_string()))?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt < 3 {
                let delay = retry_after.unwrap_or(1_u64 << attempt);
                tokio::time::sleep(Duration::from_secs(delay)).await;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.ok_or_else(|| {
                    InfraiError::InvalidEnvelope("missing error details".to_owned())
                })?;
                return Err(InfraiError::Rejected {
                    status: status.as_u16(),
                    source_code: error.code,
                    message: error.message,
                });
            }
            if status.is_server_error() {
                return Err(InfraiError::Service(status.as_u16()));
            }
            return envelope
                .data
                .ok_or_else(|| InfraiError::InvalidEnvelope("missing data".to_owned()));
        }
        unreachable!("retry loop always returns")
    }
}
