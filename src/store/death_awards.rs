use super::*;

/// One death bonus as archived and shown: (id, description, fame).
pub type Bonus = (String, String, i32);

pub(super) fn check_schema(db: &Connection, version: i32) -> Result<()> {
    let columns = {
        let mut q = db.prepare("SELECT name FROM pragma_table_xinfo('deaths') ORDER BY cid")?;
        q.query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let expected: &[&str] = if version >= 18 {
        &["character", "snapshot", "killed_by", "died_at", "bonus_fame", "bonuses"]
    } else if version >= 14 {
        &["character", "snapshot", "killed_by", "died_at", "equipment_fame"]
    } else {
        &["character", "snapshot", "killed_by", "died_at"]
    };
    ensure!(
        (version == 0 && columns.is_empty())
            || columns
                .iter()
                .map(String::as_str)
                .eq(expected.iter().copied()),
        "death award schema/version/column disagreement; restore reviewed state"
    );
    if version >= 14 {
        let reference: bool = db.query_row("SELECT count(*)=1 AND count(*) FILTER(WHERE \"table\"='characters' AND \"from\"='character' AND \"to\"='id' AND seq=0 AND on_update='NO ACTION' AND on_delete='NO ACTION')=1 FROM pragma_foreign_key_list('deaths')",[],|r|r.get(0))?;
        ensure!(reference, "unsupported death character foreign key");
        let fame = if version >= 18 { "bonus_fame" } else { "equipment_fame" };
        for (column, kind, not_null, default, key) in [
            ("character", "INTEGER", false, None, 1),
            ("snapshot", "TEXT", true, None, 0),
            ("killed_by", "TEXT", true, None, 0),
            ("died_at", "INTEGER", true, Some("unixepoch()"), 0),
            (fame, "INTEGER", true, None, 0),
        ] {
            let metadata: (String, bool, Option<String>, i32, i32) = db.query_row(
                "SELECT type,\"notnull\",dflt_value,pk,hidden FROM pragma_table_xinfo('deaths') WHERE name=?1", [column],
                |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
            ensure!(
                metadata == (kind.into(), not_null, default.map(str::to_owned), key, 0),
                "unsupported canonical death metadata: {column}"
            );
        }
    }
    Ok(())
}

pub(super) fn migrate(db: &Connection, version: i32, legacy_base: [i32; 8]) -> Result<()> {
    if version < 14 {
        // Old deaths were paid base-only; their original snapshots and wallets must not change.
        db.execute_batch("ALTER TABLE deaths RENAME TO legacy_deaths;
CREATE TABLE deaths(character INTEGER PRIMARY KEY REFERENCES characters(id),snapshot TEXT NOT NULL,killed_by TEXT NOT NULL,died_at INTEGER NOT NULL DEFAULT(unixepoch()),equipment_fame INTEGER NOT NULL CHECK(typeof(equipment_fame)='integer' AND equipment_fame BETWEEN 0 AND 2147483647));
INSERT INTO deaths(character,snapshot,killed_by,died_at,equipment_fame) SELECT character,snapshot,killed_by,died_at,0 FROM legacy_deaths;
DROP TABLE legacy_deaths;
CREATE INDEX deaths_by_time ON deaths(died_at);")?;
    }
    if version < 18 {
        // Earlier awards were equipment-only; the itemised list keeps paid amounts as they were.
        db.execute_batch("ALTER TABLE deaths RENAME COLUMN equipment_fame TO bonus_fame;
ALTER TABLE deaths ADD COLUMN bonuses TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(bonuses));
UPDATE deaths SET bonuses=json_array(json_array('Well Equipped','Bonus for equipment',bonus_fame)) WHERE bonus_fame>0;")?;
    }
    check_schema(db, 18)?;
    let mut q = db.prepare("SELECT d.character,c.account,c.alive,d.snapshot,d.bonus_fame,d.bonuses FROM deaths d JOIN characters c ON c.id=d.character")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, i32>(0)?,
            r.get::<_, i32>(1)?,
            r.get::<_, bool>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i32>(4)?,
            r.get::<_, String>(5)?,
        ))
    })? {
        let (id, account, alive, raw, bonus, bonuses) = row?;
        let ch = decode_character(&raw, legacy_base)?;
        ensure!(
            ch.id == id
                && ch.account == account
                && !alive
                && !ch.alive
                && ch.hp == 0
                && ch.items.len() == 12,
            "invalid archived death ownership or snapshot {id}"
        );
        total(&ch, bonus)?;
        let listed: Vec<Bonus> = serde_json::from_str(&bonuses)?;
        ensure!(
            listed.iter().map(|b| i64::from(b.2)).sum::<i64>() == i64::from(bonus),
            "archived death bonuses disagree with the paid award {id}"
        );
    }
    let orphan: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM deaths d LEFT JOIN characters c ON c.id=d.character WHERE c.id IS NULL)",[],|r|r.get(0))?;
    ensure!(!orphan, "orphaned death award");
    Ok(())
}

pub(super) fn validate_wallets(db: &Connection) -> Result<()> {
    let broken: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM account_fame f LEFT JOIN (SELECT c.account,sum(json_extract(d.snapshot,'$.fame')+d.bonus_fame) AS paid FROM deaths d JOIN characters c ON c.id=d.character GROUP BY c.account) d ON d.account=f.account WHERE f.earned<>coalesce(d.paid,0) OR f.balance<0 OR f.balance>f.earned OR f.earned>2147483647)",[],|r|r.get(0))?;
    ensure!(
        !broken,
        "archived death awards disagree with account fame wallets; restore reviewed state"
    );
    Ok(())
}

/// The death bonuses, in the order the May2012 calculator applies them: every percent is taken
/// from the running subtotal, Well Equipped and First Born last.
pub(super) fn bonuses(tx: &Connection, ch: &Character, content: &Content) -> Result<Vec<Bonus>> {
    ensure!(
        ch.items.len() == 12 && ch.fame >= 0,
        "invalid death equipment or base fame"
    );
    ch.effective(content)?;
    let mut percent = 0_u64;
    for kind in &ch.items[..4] {
        if *kind == -1 {
            continue;
        }
        let item = content.get(u16::try_from(*kind)?)?;
        ensure!(
            item.class == "Equipment" && !item.resurrects,
            "invalid permanent-death equipment {}",
            item.name
        );
        percent = percent
            .checked_add(u64::from(item.fame_bonus))
            .context("equipment percent overflow")?;
    }
    let (previous, best): (i64, Option<i64>) = tx.query_row(
        "SELECT count(*),max(json_extract(d.snapshot,'$.fame')+d.bonus_fame) FROM deaths d JOIN characters c ON c.id=d.character WHERE c.account=?1",
        [ch.account],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let maxed = ch.level == 20;
    let (shots, hits, kills, gods) = (ch.shots, ch.hits, ch.kills, ch.god_kills);
    let tunnel = ch.pirate_caves > 0
        && ch.dungeons.iter().all(|n| *n > 0)
        && ch.abysses > 0
        && ch.spider_dens > 0
        && ch.tombs > 0;
    let rules: [(&str, &str, bool, f64, f64); 18] = [
        ("Ancestor", "First death of any of your characters", previous == 0, 0.1, 20.),
        ("Pacifist", "Never shot a bullet which hit an enemy", hits == 0, 0.25, 0.),
        ("Thirsty", "Never drank a potion from inventory", ch.potions == 0, 0.25, 0.),
        ("Mundane", "Never used special ability (requires level 20)", maxed && ch.abilities == 0, 0.25, 0.),
        ("Boots on the Ground", "Never teleported", ch.teleports == 0, 0.25, 0.),
        ("Tunnel Rat", "Completed every dungeon type", tunnel, 0.1, 0.),
        ("Enemy of the Gods", "More than 10% of kills are gods (requires level 20)", maxed && gods * 10 > kills, 0.1, 0.),
        ("Slayer of the Gods", "More than 50% of kills are gods (requires level 20)", maxed && gods * 2 > kills, 0.1, 0.),
        ("Oryx Slayer", "Dealt killing blow to Oryx", ch.oryx_kills > 0, 0.1, 0.),
        ("Accurate", "Accuracy of better than 25% (requires level 20)", maxed && hits * 4 > shots, 0.1, 0.),
        ("Sharpshooter", "Accuracy of better than 50% (requires level 20)", maxed && hits * 2 > shots, 0.1, 0.),
        ("Sniper", "Accuracy of better than 75% (requires level 20)", maxed && hits * 4 > shots * 3, 0.1, 0.),
        ("Explorer", "More than 1 million tiles uncovered", ch.tiles > 1_000_000, 0.05, 0.),
        ("Cartographer", "More than 4 million tiles uncovered", ch.tiles > 4_000_000, 0.05, 0.),
        ("Team Player", "More than 100 party member level ups", ch.level_assists > 100, 0.1, 0.),
        ("Leader of Men", "More than 1000 party member level ups", ch.level_assists > 1000, 0.1, 0.),
        ("Doer of Deeds", "More than 1000 quests completed", ch.quests > 1000, 0.05, 0.),
        ("Friend of the Cubes", "Never killed a cube (requires level 20)", maxed && ch.cube_kills == 0, 0.05, 0.),
    ];
    let mut out = Vec::new();
    let mut subtotal = i64::from(ch.fame);
    for (id, desc, earned, pct, flat) in rules {
        if earned {
            let fame = (subtotal as f64 * pct + flat) as i64;
            award(&mut out, &mut subtotal, id, desc, fame)?;
        }
    }
    if percent > 0 {
        let fame = subtotal * i64::try_from(percent)? / 100;
        award(&mut out, &mut subtotal, "Well Equipped", "Bonus for equipment", fame)?;
    }
    if best.is_none_or(|best| subtotal > best) {
        let fame = subtotal / 10;
        award(&mut out, &mut subtotal, "First Born", "Best fame of any of your previous incarnations", fame)?;
    }
    total(ch, i32::try_from(subtotal - i64::from(ch.fame))?)?;
    Ok(out)
}

fn award(out: &mut Vec<Bonus>, subtotal: &mut i64, id: &str, desc: &str, fame: i64) -> Result<()> {
    let fame = i32::try_from(fame).context("death bonus exceeds wire range")?;
    *subtotal += i64::from(fame);
    out.push((id.into(), desc.into(), fame));
    Ok(())
}

pub(super) fn total(ch: &Character, bonus: i32) -> Result<i32> {
    ensure!(ch.fame >= 0 && bonus >= 0, "invalid archived death fame");
    ch.fame
        .checked_add(bonus)
        .context("total death fame overflow")
}
