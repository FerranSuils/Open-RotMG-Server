//! Moderation and admin chat commands, gated by the account rank (store::moderation).
use super::*;
use crate::store::moderation::{remaining, RANKS};

/// Rank needed by a command; None when it is not an admin command.
fn required(command: &str) -> Option<i32> {
    Some(match command {
        "/rank" | "/online" | "/where" => 0,
        "/announce" | "/kick" | "/ban" | "/unban" | "/banip" | "/mute" | "/unmute" => 2,
        "/setrank" | "/spawn" | "/give" | "/max" | "/killall" | "/cleargraves" | "/tppos" | "/tq" => 3,
        _ => return None,
    })
}

pub(super) fn is_command(text: &str) -> bool {
    text.split_whitespace()
        .next()
        .is_some_and(|c| required(&c.to_ascii_lowercase()).is_some())
}

/// The moderator's reply, or a rule rejection the caller shows as a chat error.
pub(super) async fn command(a: &App, owner: i32, id: i32, text: &str, received: std::time::Instant) -> Result<String> {
    crate::world::validate_chat(text)?;
    a.world.directory(move |d| d.chat_permit(id, received)).await?;
    let trimmed = text.trim();
    let (command, rest) = trimmed.split_once(char::is_whitespace).unwrap_or((trimmed, ""));
    let command = command.to_ascii_lowercase();
    let rest = rest.trim().to_owned();
    let rank = a.store.rank(owner).await?;
    ensure!(
        rank >= required(&command).context("not an admin command")?,
        lifecycle::controller("You do not have permission to use this command.")
    );
    // A moderator may only act on lower ranks, never on themselves.
    let target = async |usage: &'static str| -> Result<(i32, String, String)> {
        let (name, extra) = rest.split_once(char::is_whitespace).unwrap_or((&rest, ""));
        ensure!(!name.is_empty(), lifecycle::controller(usage));
        let target = a.store.account_named(name.into()).await?;
        ensure!(target != owner, lifecycle::controller("You cannot target yourself."));
        ensure!(a.store.rank(target).await? < rank, lifecycle::controller("That player outranks you."));
        Ok((target, name.to_owned(), extra.trim().to_owned()))
    };
    let reply = match command.as_str() {
        "/rank" => format!("Your rank: {}.", RANKS[rank as usize]),
        "/announce" => {
            ensure!(!rest.is_empty(), lifecycle::controller("Usage: /announce <text>"));
            let players = a.world.directory(move |d| Ok(d.announce(&rest))).await?;
            format!("Announced to {players} players.")
        }
        "/kick" => {
            let (target, name, _) = target("Usage: /kick <player>").await?;
            if a.kick(target).await? { format!("{name} kicked.") } else { format!("{name} is not online.") }
        }
        "/ban" | "/banip" => {
            let (target, name, extra) = target("Usage: /ban <player> [hours] [reason]").await?;
            let (hours, reason) = match extra.split_once(char::is_whitespace) {
                Some((h, r)) if h.parse::<u32>().is_ok() => (h.parse().ok(), r.trim().to_owned()),
                _ => (extra.parse::<u32>().ok(), if extra.parse::<u32>().is_ok() { String::new() } else { extra.clone() }),
            };
            let reason = if reason.is_empty() { "no reason given".to_owned() } else { reason };
            let result = a.store.ban(target, command == "/banip", hours, reason, owner).await?;
            a.kick(target).await?;
            format!("{name}: {result}")
        }
        "/unban" => {
            let (target, name, _) = target("Usage: /unban <player>").await?;
            if a.store.unban(target).await? { format!("{name} unbanned.") } else { format!("{name} was not banned.") }
        }
        "/mute" => {
            let (target, name, extra) = target("Usage: /mute <player> [minutes]").await?;
            let minutes = if extra.is_empty() { 10 } else { extra.parse().map_err(|_| lifecycle::controller("Minutes must be a number."))? };
            a.store.mute(target, minutes).await?;
            format!("{name} muted for {minutes} minutes.")
        }
        "/unmute" => {
            let (target, name, _) = target("Usage: /unmute <player>").await?;
            if a.store.unmute(target).await? { format!("{name} unmuted.") } else { format!("{name} was not muted.") }
        }
        "/setrank" => {
            let (target, name, extra) = target("Usage: /setrank <player> <0-3>").await?;
            let new: i32 = extra.parse().map_err(|_| lifecycle::controller("Usage: /setrank <player> <0-3>"))?;
            a.store.set_rank(target, new).await?;
            format!("{name} is now {}.", RANKS.get(new as usize).copied().unwrap_or("unknown"))
        }
        _ => a.world.directory(move |d| d.admin_command(id, &command, &rest, received)).await?,
    };
    tracing::warn!(account = owner, player = id, text, "admin command applied");
    Ok(reply)
}

/// The chat error a muted player gets instead of delivery.
pub(super) async fn muted(a: &App, owner: i32) -> Result<Option<String>> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
    Ok(a.store.muted(owner).await?.map(|until| format!("You are muted {}.", remaining(Some(until), now))))
}
