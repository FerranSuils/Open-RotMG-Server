//! Opt-in augment card overlay. The game page passes a random key on its /game socket; the
//! overlay script uses the same key to read its player's offer and pick a card. Any client
//! that embeds this page (browser or native WebView app) gets the same overlay; clients without
//! it keep /augment in chat and never receive the overlay's invulnerability.
use super::*;

pub(super) fn valid_key(key: &str) -> bool {
    key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit())
}
fn player(a: &App, key: &str) -> Result<i32> {
    ensure!(valid_key(key), "invalid panel key");
    a.panels
        .lock()
        .expect("panel registry poisoned")
        .get(key)
        .copied()
        .context("no game session for this panel")
}
#[derive(Deserialize)]
pub(super) struct StateQuery {
    key: String,
}
pub(super) async fn state(State(a): State<App>, Query(q): Query<StateQuery>) -> Response {
    hiscores::json_result(
        async {
            let Ok(id) = player(&a, &q.key) else {
                return Ok(serde_json::json!({"open": false}));
            };
            a.world
                .directory(move |d| {
                    Ok(d.worlds
                        .values_mut()
                        .find(|w| w.players.contains_key(&id))
                        .map_or_else(|| serde_json::json!({"open": false}), |w| w.panel_state(id)))
                })
                .await
        }
        .await,
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Choice {
    key: String,
    slot: usize,
}
pub(super) async fn choose(State(a): State<App>, Json(choice): Json<Choice>) -> Response {
    hiscores::json_result(
        async {
            let id = player(&a, &choice.key)?;
            a.world
                .directory(move |d| {
                    d.worlds
                        .values_mut()
                        .find(|w| w.players.contains_key(&id))
                        .context("player not in a world")?
                        .augment_command(id, &choice.slot.to_string())
                })
                .await?;
            Ok(serde_json::json!({"chosen": choice.slot}))
        }
        .await,
    )
}
pub(super) async fn script() -> Response {
    (
        [
            ("content-type", "text/javascript; charset=utf-8"),
            ("cache-control", "no-cache"),
        ],
        include_str!("panel.js"),
    )
        .into_response()
}
