//! Account ranks, bans (account and address), and mutes used by the in-game moderation commands.
use super::*;
use rusqlite::OptionalExtension;

pub const RANKS: [&str; 4] = ["Player", "Donator", "Moderator", "Admin"];

pub(super) fn migrate(tx: &rusqlite::Transaction) -> Result<()> {
    let ranked: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('accounts') WHERE name='rank')",
        [],
        |r| r.get(0),
    )?;
    if !ranked {
        tx.execute_batch("ALTER TABLE accounts ADD COLUMN rank INTEGER NOT NULL DEFAULT 0 CHECK(rank BETWEEN 0 AND 3);
ALTER TABLE accounts ADD COLUMN last_ip TEXT;")?;
    }
    tx.execute_batch("CREATE TABLE IF NOT EXISTS bans(account INTEGER PRIMARY KEY REFERENCES accounts(id),until INTEGER,reason TEXT NOT NULL,issued_by INTEGER NOT NULL,issued_at INTEGER NOT NULL DEFAULT(unixepoch()));
CREATE TABLE IF NOT EXISTS ip_bans(ip TEXT PRIMARY KEY,until INTEGER,reason TEXT NOT NULL,issued_by INTEGER NOT NULL,issued_at INTEGER NOT NULL DEFAULT(unixepoch()));
CREATE TABLE IF NOT EXISTS mutes(account INTEGER PRIMARY KEY REFERENCES accounts(id),until INTEGER NOT NULL);")?;
    Ok(())
}

fn denied(message: String) -> anyhow::Error {
    anyhow::Error::new(Rejected("moderation")).context(message)
}

/// "for 2h 05m" or "permanently", from a ban or mute deadline.
pub fn remaining(until: Option<i64>, now: i64) -> String {
    match until {
        None => "permanently".into(),
        Some(until) => {
            let left = (until - now).max(0);
            format!("for {}h {:02}m", left / 3600, left % 3600 / 60)
        }
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// An active ban row: (until, reason), expired rows deleted on the way.
fn active(tx: &rusqlite::Transaction, table: &str, key: &dyn rusqlite::ToSql) -> Result<Option<(Option<i64>, String)>> {
    let column = if table == "bans" { "account" } else { "ip" };
    tx.execute(&format!("DELETE FROM {table} WHERE {column}=?1 AND until IS NOT NULL AND until<=unixepoch()"), [key])?;
    Ok(tx
        .query_row(&format!("SELECT until,reason FROM {table} WHERE {column}=?1"), [key], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?)
}

impl Store {
    pub async fn rank(&self, account: i32) -> Result<i32> {
        self.call(move |db| {
            Ok(db.query_row("SELECT rank FROM accounts WHERE id=?1", [account], |r| r.get(0))?)
        })
        .await
    }
    pub async fn set_rank(&self, account: i32, rank: i32) -> Result<()> {
        ensure!((0..RANKS.len() as i32).contains(&rank), Rejected("Rank must be 0 (Player), 1 (Donator), 2 (Moderator) or 3 (Admin)."));
        self.call(move |db| {
            ensure!(db.execute("UPDATE accounts SET rank=?2 WHERE id=?1", params![account, rank])? == 1, Rejected("Player not found."));
            tracing::warn!(account, rank, "account rank changed");
            Ok(())
        })
        .await
    }
    /// Records the login address and refuses banned accounts and addresses.
    pub async fn login(&self, account: i32, ip: Option<std::net::IpAddr>) -> Result<()> {
        let ip = ip.map(|ip| ip.to_string());
        self.call(move |db| {
            let tx = db.transaction()?;
            if let Some((until, reason)) = active(&tx, "bans", &account)? {
                tracing::warn!(account, "banned account refused");
                return Err(denied(format!("Your account is banned {}: {reason}", remaining(until, now()))));
            }
            if let Some(ip) = &ip {
                if let Some((until, reason)) = active(&tx, "ip_bans", ip)? {
                    tracing::warn!(account, ip, "banned address refused");
                    return Err(denied(format!("Your address is banned {}: {reason}", remaining(until, now()))));
                }
                tx.execute("UPDATE accounts SET last_ip=?2 WHERE id=?1", params![account, ip])?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }
    /// Bans the account, and with `address` its last login address too. `hours` of None is permanent.
    pub async fn ban(&self, account: i32, address: bool, hours: Option<u32>, reason: String, by: i32) -> Result<String> {
        ensure!(hours != Some(0), Rejected("Ban length must be at least one hour."));
        let until = hours.map(|h| now() + i64::from(h) * 3600);
        self.call(move |db| {
            let tx = db.transaction()?;
            tx.execute("INSERT OR REPLACE INTO bans(account,until,reason,issued_by) VALUES(?1,?2,?3,?4)", params![account, until, reason, by])?;
            let mut banned = format!("account {account}");
            if address {
                let ip: Option<String> = tx.query_row("SELECT last_ip FROM accounts WHERE id=?1", [account], |r| r.get(0))?;
                let ip = ip.ok_or_else(|| Rejected("That player has no recorded address."))?;
                tx.execute("INSERT OR REPLACE INTO ip_bans(ip,until,reason,issued_by) VALUES(?1,?2,?3,?4)", params![ip, until, reason, by])?;
                banned = format!("{banned} and address {ip}");
            }
            tx.commit()?;
            tracing::warn!(account, by, address, ?until, %reason, "ban issued");
            Ok(format!("Banned {banned} {}.", remaining(until, now())))
        })
        .await
    }
    pub async fn unban(&self, account: i32) -> Result<bool> {
        self.call(move |db| {
            let tx = db.transaction()?;
            let n = tx.execute("DELETE FROM bans WHERE account=?1", [account])?
                + tx.execute("DELETE FROM ip_bans WHERE ip=(SELECT last_ip FROM accounts WHERE id=?1)", [account])?;
            tx.commit()?;
            tracing::warn!(account, rows = n, "ban lifted");
            Ok(n > 0)
        })
        .await
    }
    pub async fn mute(&self, account: i32, minutes: u32) -> Result<()> {
        ensure!(minutes > 0, Rejected("Mute length must be at least one minute."));
        let until = now() + i64::from(minutes) * 60;
        self.call(move |db| {
            db.execute("INSERT OR REPLACE INTO mutes(account,until) VALUES(?1,?2)", params![account, until])?;
            tracing::warn!(account, until, "mute issued");
            Ok(())
        })
        .await
    }
    pub async fn unmute(&self, account: i32) -> Result<bool> {
        self.call(move |db| Ok(db.execute("DELETE FROM mutes WHERE account=?1", [account])? > 0)).await
    }
    /// The mute deadline when the account is muted now.
    pub async fn muted(&self, account: i32) -> Result<Option<i64>> {
        self.call(move |db| {
            db.execute("DELETE FROM mutes WHERE account=?1 AND until<=unixepoch()", [account])?;
            Ok(db.query_row("SELECT until FROM mutes WHERE account=?1", [account], |r| r.get(0)).optional()?)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ranks_bans_and_mutes_apply_and_expire() {
        let content = Content::load(Path::new("data")).unwrap();
        let s = Store::open(Path::new(":memory:"), &content).unwrap();
        let a = s.list("mod-a".into()).await.unwrap().id;
        let b = s.list("mod-b".into()).await.unwrap().id;
        assert_eq!(s.rank(a).await.unwrap(), 0);
        s.set_rank(a, 3).await.unwrap();
        assert_eq!(s.rank(a).await.unwrap(), 3);
        assert!(s.set_rank(a, 4).await.is_err());
        let ip: std::net::IpAddr = "203.0.113.7".parse().unwrap();
        s.login(b, Some(ip)).await.unwrap();
        let reply = s.ban(b, true, Some(2), "spam".into(), a).await.unwrap();
        assert!(reply.contains("203.0.113.7") && reply.contains("for 1h 59m") || reply.contains("for 2h 00m"), "{reply}");
        let error = s.login(b, None).await.unwrap_err();
        assert!(crate::server::lifecycle::rule_rejection(&error) && error.to_string().contains("spam"));
        // The address ban also stops other accounts from the same address.
        assert!(s.login(a, Some(ip)).await.unwrap_err().to_string().contains("address"));
        assert!(s.unban(b).await.unwrap());
        s.login(b, Some(ip)).await.unwrap();
        s.login(a, Some(ip)).await.unwrap();
        // Expired bans lift themselves.
        s.ban(b, false, None, "forever".into(), a).await.unwrap();
        s.call(move |db| { db.execute("UPDATE bans SET until=unixepoch()-1 WHERE account=?1", [b])?; Ok(()) }).await.unwrap();
        s.login(b, None).await.unwrap();
        assert!(s.muted(b).await.unwrap().is_none());
        s.mute(b, 5).await.unwrap();
        assert!(s.muted(b).await.unwrap().is_some());
        assert!(s.unmute(b).await.unwrap());
        assert!(s.muted(b).await.unwrap().is_none());
    }
}
