//! Live standings (refreshed every two seconds) and the public fame boards: real time, weekly,
//! monthly and yearly (calendar periods in UTC).
use super::*;
use crate::regalia::{Period, RANKS, Standings};

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
async fn living_scores(a: &App) -> Result<serde_json::Value> {
    let living = a
        .world
        .directory(|d| Ok(d.competitive_characters()))
        .await?;
    a.store.hiscores(living).await
}
fn pairs(scores: &serde_json::Value) -> Result<Vec<(i32, i64, String)>> {
    scores["players"]
        .as_array()
        .context("hiscores without players")?
        .iter()
        .map(|p| {
            Ok((
                p["account"].as_i64().context("account")? as i32,
                p["base_fame"].as_i64().context("living fame")?,
                p["name"].as_str().context("name")?.to_owned(),
            ))
        })
        .collect()
}
/// Recomputes standings, persists crown changes and hands the result to every world.
pub(super) async fn standings_task(a: App) -> Result<()> {
    let mut current = Standings::default();
    if let Some((leader, since)) = a.store.crown_reign().await? {
        current.leader = Some(leader);
        current.reign_since = since;
    }
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let scores: Vec<(i32, i64)> = pairs(&living_scores(&a).await?)?
            .into_iter()
            .map(|(account, fame, _)| (account, fame))
            .collect();
        let next = crate::regalia::standings(&scores, &current, unix_now());
        if next.leader != current.leader {
            a.store
                .set_crown_reign(next.leader.map(|leader| (leader, next.reign_since)))
                .await?;
            tracing::info!(leader = ?next.leader, previous = ?current.leader, "server crown changed hands");
        }
        if next != current {
            let shared = Arc::new(next.clone());
            a.world
                .directory(move |d| {
                    d.standings = shared;
                    Ok(())
                })
                .await?;
        }
        current = next;
    }
}
#[derive(Deserialize)]
pub(super) struct Query {
    #[serde(default)]
    period: String,
}
pub(super) async fn board(
    State(a): State<App>,
    axum::extract::Query(q): axum::extract::Query<Query>,
) -> Response {
    hiscores::json_result(
        async {
            let now = unix_now();
            let standings = a.world.directory(|d| Ok(d.standings.clone())).await?;
            let period = match q.period.as_str() {
                "" | "realtime" => None,
                "week" => Some(Period::Week),
                "month" => Some(Period::Month),
                "year" => Some(Period::Year),
                other => anyhow::bail!("unknown period {other}"),
            };
            let mut rows: Vec<(i32, String, i64)> = match period {
                None => pairs(&living_scores(&a).await?)?
                    .into_iter()
                    .map(|(account, fame, name)| (account, name, fame))
                    .collect(),
                Some(p) => a.store.fame_board(crate::regalia::period_start(p, now), 100).await?,
            };
            rows.sort_by(|x, y| y.2.cmp(&x.2).then(x.0.cmp(&y.0)));
            rows.truncate(100);
            let players: Vec<_> = rows
                .iter()
                .enumerate()
                .map(|(i, (account, name, fame))| {
                    let rank = standings.positions.get(account).map(|n| &RANKS[crate::regalia::rank(*n)]);
                    serde_json::json!({"position": i + 1, "name": name, "fame": fame,
                        "rank": rank.map(|r| r.name), "rank_color": rank.map(|r| format!("#{:06x}", r.color)),
                        "crown": standings.leader == Some(*account)})
                })
                .collect();
            let reign = standings.leader.map(|_| now.saturating_sub(standings.reign_since));
            Ok(serde_json::json!({
                "period": period.map_or("realtime", |p| match p { Period::Week => "week", Period::Month => "month", Period::Year => "year" }),
                "since": period.map(|p| crate::regalia::period_start(p, now)),
                "updated_at": now,
                "reign_secs": reign,
                "crown_level": reign.map(crate::regalia::crown_level),
                "ranks": RANKS.iter().map(|r| serde_json::json!({"name": r.name, "last": r.last, "color": format!("#{:06x}", r.color)})).collect::<Vec<_>>(),
                "players": players,
            }))
        }
        .await,
    )
}
pub(super) async fn page() -> Response {
    (
        [
            ("content-type", "text/html; charset=utf-8"),
            ("cache-control", "no-store"),
        ],
        include_str!("ranking.html"),
    )
        .into_response()
}
