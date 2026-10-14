use super::hiscores::{authorize, json_result};
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DiscordRequest {
    discord_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoverRequest {
    discord_id: String,
    code: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResetRequest {
    token: String,
    password: String,
}
fn private_denial(a: &App, peer: SocketAddr, headers: &HeaderMap) -> Option<Response> {
    if authorize(a, peer, headers).is_err() {
        return Some(
            (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error":"account API authorization denied"})),
            )
                .into_response(),
        );
    }
    if !rate_allow(
        &mut a.requests.lock().expect("request registry poisoned"),
        peer.ip(),
        std::time::Instant::now(),
    ) {
        return Some(
            (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({"error":"account attempt limit; wait one minute"})),
            )
                .into_response(),
        );
    }
    None
}
pub(super) async fn discord_kick(
    State(a): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<DiscordRequest>,
) -> Response {
    if let Some(response) = private_denial(&a, peer, &headers) {
        return response;
    }
    json_result(
        async {
            let account = a.store.linked_account(request.discord_id).await?;
            let disconnected = a.kick(account).await?;
            tracing::info!(account, disconnected, "self-disconnect request completed");
            Ok(serde_json::json!({"disconnected":disconnected}))
        }
        .await,
    )
}
pub(super) async fn discord_recover(
    State(a): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<RecoverRequest>,
) -> Response {
    if let Some(response) = private_denial(&a, peer, &headers) {
        return response;
    }
    json_result(
        a.store
            .issue_reset(request.discord_id, request.code)
            .await
            .map(|token| serde_json::json!({"url":format!("{}/recover/#{token}",a.origin)})),
    )
}
pub(super) async fn reset_password(
    State(a): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<ResetRequest>,
) -> Response {
    if headers.get("origin").and_then(|h| h.to_str().ok()) != Some(a.origin.as_str()) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"reset request origin denied"})),
        )
            .into_response();
    }
    match admit(&a, &headers, peer) {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({"error":"Too many reset attempts; wait one minute"})),
            )
                .into_response();
        }
        Err(e) => return json_result(Err(e)),
    }
    json_result(
        async {
            a.store.reset_account(request.token.clone()).await?;
            let verifier = a.store.prepare_password(request.password).await?;
            // Fence authentication through its ownership claim, not just the credential lookup.
            let _authentication = a.authentication.write().await;
            let account = a.store.reset_account(request.token.clone()).await?;
            a.kick(account).await?;
            ensure!(
                !a.owners
                    .lock()
                    .expect("ownership registry poisoned")
                    .contains_key(&account),
                "account ownership was not released"
            );
            a.store.finish_reset(request.token, verifier).await?;
            Ok(serde_json::json!({"reset":true}))
        }
        .await,
    )
}

#[cfg(test)]
pub(crate) mod tests;
