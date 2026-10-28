//! Server-attributable death reversal. Every death stores the evidence the server had at that
//! moment; an operator can return the character with everything it carried only when that
//! evidence shows a server fault. Client-side causes (lost connection, lag) never qualify.
use super::*;
use rusqlite::OptionalExtension;

/// A simulation stall at least this long in the five seconds before death is a server fault.
pub const STALL_MS: u64 = 500;

pub fn server_fault(evidence: &serde_json::Value) -> std::result::Result<String, String> {
    let stall = evidence["server_stall_ms"]
        .as_u64()
        .ok_or("death has no recorded server evidence")?;
    let input = evidence["client_input_age_ms"].as_u64().unwrap_or(0);
    if stall >= STALL_MS {
        return Ok(format!("server stalled {stall} ms in the 5 s before death"));
    }
    if input >= 1000 {
        return Err(format!(
            "server was healthy (worst stall {stall} ms) and the client sent no movement for {input} ms: player connection fault"
        ));
    }
    Err(format!(
        "server was running normally (worst stall {stall} ms)"
    ))
}

pub(super) fn migrate(tx: &rusqlite::Transaction, version: i32) -> Result<()> {
    if version < 17 {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS death_evidence(id INTEGER PRIMARY KEY,character INTEGER NOT NULL REFERENCES characters(id),died_at INTEGER NOT NULL,award INTEGER NOT NULL CHECK(award>=0),guild INTEGER REFERENCES guilds(id),evidence TEXT NOT NULL CHECK(json_valid(evidence)),restored_at INTEGER,restored_reason TEXT);
CREATE INDEX IF NOT EXISTS death_evidence_by_character ON death_evidence(character);")?;
    }
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='death_evidence')",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        exists,
        "schema-17 death_evidence missing; restore reviewed state"
    );
    Ok(())
}
/// Stores the death's evidence with the exact award and guild credit it paid, inside the death transaction.
pub(super) fn record(
    tx: &rusqlite::Transaction,
    ch: &Character,
    award: i32,
    evidence: &serde_json::Value,
) -> Result<()> {
    let guild = guilds::membership(tx, ch.account)?.map(|m| m.id);
    tx.execute(
        "INSERT INTO death_evidence(character,died_at,award,guild,evidence) VALUES(?1,unixepoch(),?2,?3,?4)",
        params![ch.id, award, guild, evidence.to_string()],
    )?;
    Ok(())
}
impl Store {
    pub async fn character_account(&self, character: i32) -> Result<i32> {
        self.call(move |db| {
            db.query_row(
                "SELECT account FROM characters WHERE id=?1",
                [character],
                |r| r.get(0),
            )
            .optional()?
            .context("unknown character")
        })
        .await
    }
    /// Most recent deaths with their evidence and whether the server is at fault.
    pub async fn recent_deaths(&self, limit: u32) -> Result<Vec<serde_json::Value>> {
        self.call(move |db| {
            let mut q = db.prepare("SELECT e.character,c.account,a.name,e.died_at,e.award,e.evidence,e.restored_at,e.restored_reason FROM death_evidence e JOIN characters c ON c.id=e.character JOIN accounts a ON a.id=c.account ORDER BY e.id DESC LIMIT ?1")?;
            let rows = q.query_map([limit], |r| {
                Ok((r.get::<_, i32>(0)?, r.get::<_, i32>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, i32>(4)?,
                    r.get::<_, String>(5)?, r.get::<_, Option<i64>>(6)?, r.get::<_, Option<String>>(7)?))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (character, account, name, died_at, award, evidence, restored_at, reason) = row?;
                let evidence: serde_json::Value = serde_json::from_str(&evidence)?;
                let verdict = server_fault(&evidence);
                out.push(serde_json::json!({"character": character, "account": account, "name": name, "died_at": died_at,
                    "award": award, "evidence": evidence, "restored_at": restored_at, "restored_reason": reason,
                    "server_fault": verdict.is_ok(), "verdict": verdict.unwrap_or_else(|why| why)}));
            }
            Ok(out)
        })
        .await
    }
    /// Revives a character to its death snapshot (stats, items, augments, counters) and reverses the
    /// death award, refusing unless the stored evidence shows a server fault.
    pub async fn restore_death(&self, character: i32, operator: String) -> Result<(i32, String)> {
        let (content, legacy_base) = (self.content.clone(), self.legacy_base);
        self.call(move |db| {
            let tx = db.transaction()?;
            let (evidence_id, award, guild, evidence, restored): (i64, i32, Option<i32>, String, Option<i64>) = tx
                .query_row("SELECT id,award,guild,evidence,restored_at FROM death_evidence WHERE character=?1 ORDER BY id DESC LIMIT 1",
                    [character], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
                .optional()?
                .context("this character has no death with recorded evidence")?;
            ensure!(restored.is_none(), "this death was already restored");
            let reason = server_fault(&serde_json::from_str(&evidence)?)
                .map_err(|why| anyhow::anyhow!("not restored: {why}"))?;
            let snapshot: String = tx
                .query_row("SELECT snapshot FROM deaths WHERE character=?1", [character], |r| r.get(0))
                .optional()?
                .context("the death is no longer archived")?;
            let (account, revision, alive): (i32, i32, bool) = tx.query_row(
                "SELECT account,revision,alive FROM characters WHERE id=?1", [character], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            ensure!(!alive, "the character is already alive");
            let mut ch = decode_character(&snapshot, legacy_base)?;
            ensure!(ch.id == character && ch.account == account && ch.items.len() == 12, "inconsistent death snapshot");
            ch.alive = true;
            ch.revision = revision + 1;
            let stats = ch.effective(&content)?;
            (ch.hp, ch.mp) = (stats[0], stats[1]);
            ensure!(
                tx.execute("UPDATE account_fame SET balance=balance-?2,earned=earned-?2 WHERE account=?1 AND balance>=?2", params![account, award])? == 1,
                "the account already spent the {award} fame this death paid; the award cannot be undone"
            );
            if let Some(guild) = guild.filter(|g| tx.query_row("SELECT EXISTS(SELECT 1 FROM guilds WHERE id=?1)", [g], |r| r.get::<_, bool>(0)).unwrap_or(false)) {
                ensure!(
                    tx.execute("UPDATE guilds SET fame=fame-?2,earned=earned-?2 WHERE id=?1 AND fame>=?2", params![guild, award])? == 1,
                    "the guild already spent the fame this death contributed"
                );
                tx.execute("UPDATE guild_members SET fame=max(0,fame-?3) WHERE account=?1 AND guild=?2", params![account, guild, award])?;
            }
            let n = tx
                .execute("UPDATE characters SET alive=1,stats=?3,items=?4,revision=?5 WHERE id=?1 AND account=?2 AND alive=0 AND revision=?6",
                    params![character, account, serde_json::to_string(&ch)?, serde_json::to_string(&ch.items)?, ch.revision, revision])
                .map_err(|e| anyhow::anyhow!("no free character slot on the account ({e})"))?;
            ensure!(n == 1, "the character changed during restoration");
            tx.execute("DELETE FROM deaths WHERE character=?1", [character])?;
            tx.execute("UPDATE death_evidence SET restored_at=unixepoch(),restored_reason=?2 WHERE id=?1",
                params![evidence_id, format!("{reason} · {operator}")])?;
            tx.commit()?;
            tracing::warn!(character, account, award, %reason, %operator, "server-fault death reversed; character restored");
            Ok((account, reason))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn dead(
        s: &Store,
        content: &Content,
        guid: &str,
        evidence: serde_json::Value,
    ) -> (i32, Character) {
        let account = s.list(guid.into()).await.unwrap();
        let mut ch = s
            .create(account.id, content.get(WIZARD).unwrap())
            .await
            .unwrap();
        ch.exp = 120_000;
        ch.fame = 120;
        ch.augments = vec![0, 0, 2];
        s.progress(ch.clone()).await.unwrap();
        let alive = ch.clone();
        ch.alive = false;
        ch.hp = 0;
        s.death(ch, "Wandering Pirate".into(), evidence)
            .await
            .unwrap();
        (account.id, alive)
    }
    fn wallet(s: &Store, account: i32) -> impl std::future::Future<Output = (i64, i64)> + '_ {
        s.call(move |db| {
            Ok(db.query_row(
                "SELECT balance,earned FROM account_fame WHERE account=?1",
                [account],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .map(|r| r.unwrap())
    }
    use futures_util::FutureExt;

    #[tokio::test]
    async fn server_stall_deaths_are_reversed_with_everything_the_character_had() {
        let content = Content::load(Path::new("data")).unwrap();
        let s = Store::open(Path::new(":memory:"), &content).unwrap();
        let stall = serde_json::json!({"server_stall_ms": 900, "client_input_age_ms": 40, "killed_by": "Wandering Pirate"});
        let (account, alive) = dead(&s, &content, "restore-ok", stall).await;
        let (balance, earned) = wallet(&s, account).await;
        assert!(
            balance == earned && balance > 120,
            "death paid its award with bonuses: {balance}/{earned}"
        );
        let listed = s.recent_deaths(10).await.unwrap();
        assert_eq!(listed[0]["server_fault"], true);
        let (owner, reason) = s
            .restore_death(alive.id, "operator: test".into())
            .await
            .unwrap();
        assert_eq!(owner, account);
        assert!(reason.contains("900 ms"));
        let back = s.load(account, alive.id).await.unwrap();
        assert!(back.alive);
        assert_eq!(
            (
                back.items.clone(),
                back.fame,
                back.augments.clone(),
                back.level
            ),
            (alive.items, alive.fame, alive.augments, alive.level)
        );
        assert_eq!(
            wallet(&s, account).await,
            (0, 0),
            "the death award is taken back"
        );
        assert!(
            s.restore_death(alive.id, "again".into()).await.is_err(),
            "a death is reversed once"
        );
        assert_eq!(
            s.recent_deaths(10).await.unwrap()[0]["restored_at"].is_i64(),
            true
        );
    }

    #[tokio::test]
    async fn client_faults_healthy_servers_and_spent_awards_are_refused() {
        let content = Content::load(Path::new("data")).unwrap();
        let s = Store::open(Path::new(":memory:"), &content).unwrap();
        let (_, lag) = dead(
            &s,
            &content,
            "client-lag",
            serde_json::json!({"server_stall_ms": 20, "client_input_age_ms": 3000}),
        )
        .await;
        let error = s
            .restore_death(lag.id, "x".into())
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("player connection fault"), "{error}");
        let (_, healthy) = dead(
            &s,
            &content,
            "healthy",
            serde_json::json!({"server_stall_ms": 60, "client_input_age_ms": 50}),
        )
        .await;
        assert!(
            s.restore_death(healthy.id, "x".into())
                .await
                .unwrap_err()
                .to_string()
                .contains("running normally")
        );
        let (spender, spent) = dead(
            &s,
            &content,
            "spender",
            serde_json::json!({"server_stall_ms": 700}),
        )
        .await;
        s.call(move |db| {
            db.execute(
                "UPDATE account_fame SET balance=0 WHERE account=?1",
                [spender],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        assert!(
            s.restore_death(spent.id, "x".into())
                .await
                .unwrap_err()
                .to_string()
                .contains("already spent")
        );
        let (busy, replaced) = dead(
            &s,
            &content,
            "slot-taken",
            serde_json::json!({"server_stall_ms": 700}),
        )
        .await;
        s.create(busy, content.get(WIZARD).unwrap()).await.unwrap();
        assert!(
            s.restore_death(replaced.id, "x".into())
                .await
                .unwrap_err()
                .to_string()
                .contains("free character slot")
        );
        assert!(
            !s.load(busy, replaced.id).await.is_ok_and(|c| c.alive),
            "refusals change nothing"
        );
    }
}
