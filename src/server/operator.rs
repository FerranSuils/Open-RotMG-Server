//! Operator-only actions from the server machine itself: loopback requests carrying the token
//! in STATE/operator.token. Reverses deaths only when their evidence shows a server fault.
use super::*;
use sha2::{Digest, Sha256};

/// Reads the operator credential, creating it on first start.
pub(crate) fn token(state: &std::path::Path) -> Result<String> {
    let path = state.join("operator.token");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim().to_string();
        ensure!(
            existing.len() == 64 && existing.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid operator token file"
        );
        return Ok(existing);
    }
    let token = (0..32)
        .map(|_| format!("{:02x}", rand::random::<u8>()))
        .collect::<String>();
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    options.open(&path)?.write_all(token.as_bytes())?;
    tracing::info!(token_file=%path.display(), "operator credential created");
    Ok(token)
}
fn authorize(a: &App, peer: SocketAddr, headers: &HeaderMap) -> Result<()> {
    ensure!(
        peer.ip().is_loopback()
            && ![
                "x-forwarded-for",
                "x-forwarded-host",
                "x-forwarded-proto",
                "forwarded",
                "x-real-ip",
                "origin"
            ]
            .iter()
            .any(|key| headers.contains_key(*key)),
        "operator API is loopback-only"
    );
    let supplied = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .context("operator authorization required")?;
    let (left, right) = (
        Sha256::digest(a.operator_token.as_bytes()),
        Sha256::digest(supplied.as_bytes()),
    );
    ensure!(
        left.iter()
            .zip(right.iter())
            .fold(0u8, |n, (x, y)| n | (x ^ y))
            == 0,
        "invalid operator authorization"
    );
    Ok(())
}
pub(super) async fn deaths(
    State(a): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize(&a, peer, &headers) {
        return hiscores::json_error(StatusCode::FORBIDDEN, error);
    }
    hiscores::json_result(
        a.store
            .recent_deaths(50)
            .await
            .map(|rows| serde_json::json!({"deaths": rows})),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Rank {
    name: String,
    rank: i32,
}
/// Bootstraps the first admin; later ranks are set in game with /setrank.
pub(super) async fn rank(
    State(a): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<Rank>,
) -> Response {
    if let Err(error) = authorize(&a, peer, &headers) {
        return hiscores::json_error(StatusCode::FORBIDDEN, error);
    }
    hiscores::json_result(
        async {
            let account = a.store.account_named(request.name.clone()).await?;
            a.store.set_rank(account, request.rank).await?;
            Ok(serde_json::json!({"account": account, "rank": request.rank}))
        }
        .await,
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Restore {
    character: i32,
    reason: String,
}
pub(super) async fn restore(
    State(a): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<Restore>,
) -> Response {
    if let Err(error) = authorize(&a, peer, &headers) {
        return hiscores::json_error(StatusCode::FORBIDDEN, error);
    }
    hiscores::json_result(
        async {
            ensure!(!request.reason.trim().is_empty(), "a reason for the restoration is required");
            let account = a.store.character_account(request.character).await?;
            let online = a
                .world
                .directory(move |d| {
                    Ok(d.worlds
                        .values()
                        .flat_map(|w| w.players.values())
                        .any(|p| p.character.account == account))
                })
                .await?;
            ensure!(!online, "the player is online; restore once they leave the game");
            // Fence the account like other account actions so no live session races the revival.
            let (account, reason) = lifecycle::account(&a, vec![account], "restore_death", move |a| async move {
                lifecycle::stage(Stage::Store);
                a.store.restore_death(request.character, request.reason).await
            })
            .await?;
            Ok(serde_json::json!({"restored": request.character, "account": account, "reason": reason}))
        }
        .await,
    )
}
