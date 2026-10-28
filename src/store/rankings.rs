//! Fame earned over time (weekly, monthly and yearly boards) and the persisted crown reign.
use super::*;
use rusqlite::OptionalExtension;

pub(super) fn migrate(tx: &rusqlite::Transaction, version: i32) -> Result<()> {
    if version < 16 {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS fame_log(account INTEGER NOT NULL REFERENCES accounts(id),character INTEGER NOT NULL REFERENCES characters(id),amount INTEGER NOT NULL CHECK(amount>0),earned_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS fame_log_by_time ON fame_log(earned_at);
CREATE TABLE IF NOT EXISTS crown_reign(id INTEGER PRIMARY KEY CHECK(id=1),account INTEGER NOT NULL REFERENCES accounts(id),since INTEGER NOT NULL);")?;
    }
    for table in ["fame_log", "crown_reign"] {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
            [table],
            |r| r.get(0),
        )?;
        ensure!(exists, "schema-16 {table} missing; restore reviewed state");
    }
    Ok(())
}
/// Records character fame gained since the stored snapshot, inside the caller's save transaction.
pub(super) fn log_gain(tx: &rusqlite::Transaction, ch: &Character, saved_fame: i32) -> Result<()> {
    if ch.fame > saved_fame {
        tx.execute(
            "INSERT INTO fame_log(account,character,amount,earned_at) VALUES(?1,?2,?3,unixepoch())",
            params![ch.account, ch.id, ch.fame - saved_fame],
        )?;
    }
    Ok(())
}
impl Store {
    /// Top competitors by fame earned since `since` (unix seconds).
    pub async fn fame_board(&self, since: u64, limit: u32) -> Result<Vec<(i32, String, i64)>> {
        self.call(move |db| {
            let mut q = db.prepare("SELECT f.account,a.name,SUM(f.amount) AS fame FROM fame_log f JOIN accounts a ON a.id=f.account JOIN competitors c ON c.account=f.account WHERE f.earned_at>=?1 AND NOT EXISTS(SELECT 1 FROM shore_bots b WHERE b.account=f.account) GROUP BY f.account ORDER BY fame DESC,f.account LIMIT ?2")?;
            let rows = q
                .query_map(params![since as i64, limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }
    pub async fn crown_reign(&self) -> Result<Option<(i32, u64)>> {
        self.call(|db| {
            Ok(db
                .query_row(
                    "SELECT account,since FROM crown_reign WHERE id=1",
                    [],
                    |r| Ok((r.get::<_, i32>(0)?, r.get::<_, i64>(1)? as u64)),
                )
                .optional()?)
        })
        .await
    }
    pub async fn set_crown_reign(&self, reign: Option<(i32, u64)>) -> Result<()> {
        self.call(move |db| {
            match reign {
                Some((account, since)) => db.execute(
                    "INSERT INTO crown_reign(id,account,since) VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET account=excluded.account,since=excluded.since",
                    params![account, since as i64],
                )?,
                None => db.execute("DELETE FROM crown_reign", [])?,
            };
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fame_gains_feed_period_boards_and_the_crown_persists() {
        let content = Content::load(Path::new("data")).unwrap();
        let s = Store::open(Path::new(":memory:"), &content).unwrap();
        let account = s.list("ranking-board".into()).await.unwrap();
        let id = account.id;
        let mut ch = s.create(id, content.get(WIZARD).unwrap()).await.unwrap();
        s.call(move |db| {
            db.execute("INSERT INTO competitors(account) VALUES(?1)", [id])?;
            Ok(())
        })
        .await
        .unwrap();
        for fame in [40, 55, 55] {
            ch.fame = fame;
            s.progress(ch.clone()).await.unwrap();
        }
        let board = s.fame_board(0, 10).await.unwrap();
        assert_eq!(board.len(), 1);
        assert_eq!((board[0].0, board[0].2), (id, 55), "only gains are logged");
        assert!(s.fame_board(u64::MAX / 4, 10).await.unwrap().is_empty());
        assert_eq!(s.crown_reign().await.unwrap(), None);
        s.set_crown_reign(Some((id, 123))).await.unwrap();
        assert_eq!(s.crown_reign().await.unwrap(), Some((id, 123)));
        s.set_crown_reign(None).await.unwrap();
        assert_eq!(s.crown_reign().await.unwrap(), None);
    }
}
