use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use verified_domain_workspace_join::{
    infrai_client::{InfraiClient, InfraiError},
    workspace_join::{join_workspace, JoinError, JoinRequest, JoinResult},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = InfraiClient::from_env()?;
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/workspace/join", post(handle_join))
        .with_state(client);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("workspace join service listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_join(
    State(client): State<InfraiClient>,
    Json(request): Json<JoinRequest>,
) -> Result<Json<JoinResult>, ServiceError> {
    join_workspace(&client, &request)
        .await
        .map(Json)
        .map_err(ServiceError)
}

struct ServiceError(JoinError);

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            JoinError::EmailDomainMismatch | JoinError::DomainUnverified => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            JoinError::Infrai(InfraiError::Rejected { status, .. }) => {
                StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_REQUEST)
            }
            JoinError::Infrai(_) => StatusCode::BAD_GATEWAY,
        };
        (
            status,
            Json(json!({ "ok": false, "error": self.0.to_string() })),
        )
            .into_response()
    }
}
