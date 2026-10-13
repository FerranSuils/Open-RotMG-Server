use crate::content::{Content, Object, WIZARD};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::mpsc};
use tokio::sync::oneshot;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Character {
    pub schema: u8,
    pub class: u16,
    pub base: [i32; 8],
    pub id: i32,
    pub account: i32,
    pub name: String,
    pub level: i32,
    pub exp: i32,
    pub fame: i32,
    pub hp: i32,
    pub mp: i32,
    pub items: Vec<i32>,
    pub revision: i32,
    pub alive: bool,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub shots: u64,
    #[serde(default)]
    pub hits: u64,
    #[serde(default)]
    pub kills: u64,
    #[serde(default)]
    pub quests: u64,
    #[serde(default)]
    pub tiles: u64,
    #[serde(default)]
    pub pirate_caves: u64,
}
pub fn level_start(level: i32) -> Result<i32> {
    ensure!((1..=20).contains(&level), "invalid character level {level}");
    Ok(50 * (level - 1).pow(2))
}
pub fn experience_goal(level: i32) -> Result<i32> {
    level_start(level)?;
    Ok(50 + (level - 1) * 100)
}
impl Character {
    pub fn gain_experience(
        &mut self,
        content: &Content,
        amount: i32,
        rng: &mut impl rand::Rng,
    ) -> Result<bool> {
        self.effective(content)?;
        ensure!(amount >= 0, "negative experience reward");
        let mut next = self.clone();
        next.exp = next
            .exp
            .checked_add(amount)
            .context("experience overflow")?;
        let class = &content.classes[&next.class];
        let previous = next.level;
        while next.level < 20 && next.exp >= level_start(next.level + 1)? {
            next.level += 1;
            for (i, (min, max)) in class.growth.iter().enumerate() {
                next.base[i] = next.base[i]
                    .checked_add(rng.gen_range(*min..=*max))
                    .context("growth overflow")?
                    .min(class.caps[i]);
            }
        }
        if next.level > previous {
            let effective = next.effective(content)?;
            next.hp = effective[0];
            next.mp = effective[1];
        }
        *self = next;
        Ok(self.level > previous)
    }
    pub fn effective(&self, content: &Content) -> Result<[i32; 8]> {
        let class = content
            .classes
            .get(&self.class)
            .context("unknown character class")?;
        level_start(self.level)?;
        ensure!(
            self.schema == 1 && self.items.len() == 12,
            "invalid character schema or inventory"
        );
        ensure!(
            self.base
                .iter()
                .zip(class.caps)
                .all(|(n, cap)| *n >= 0 && *n <= cap),
            "invalid base stats"
        );
        let slots = &content.get(self.class)?.slots;
        let mut stats = self.base;
        for (slot, item) in self.items[..4].iter().enumerate() {
            if *item == -1 {
                continue;
            }
            let item = content.get(u16::try_from(*item)?)?;
            ensure!(
                item.slot_type == slots[slot] && !item.unsupported_equipment,
                "unsupported or incompatible equipped item {} in slot {slot}",
                item.name
            );
            for (id, amount) in &item.bonuses {
                let index = crate::content::STATS
                    .iter()
                    .position(|(stat, _)| stat == id)
                    .context("unsupported equipment stat")?;
                stats[index] = stats[index]
                    .checked_add(*amount)
                    .context("effective stat overflow")?;
            }
        }
        ensure!(stats.iter().all(|v| *v >= 0), "negative effective stat");
        Ok(stats)
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Account {
    pub id: i32,
    pub name: String,
    pub next_id: i32,
    pub best_levels: std::collections::BTreeMap<u16, i32>,
    pub characters: Vec<Character>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Vault {
    pub account: i32,
    pub items: Vec<i32>,
    pub revision: i32,
}
#[derive(Clone, Debug)]
pub struct InventoryWrite {
    pub id: i32,
    pub account: i32,
    pub revision: i32,
    pub items: Vec<i32>,
}
type Job = Box<dyn FnOnce(&mut Connection) + Send>;
#[derive(Clone)]
pub struct Store {
    tx: mpsc::Sender<Job>,
    legacy_base: [i32; 8],
}
fn decode_character(raw: &str, legacy_base: [i32; 8]) -> Result<Character> {
    let mut value: serde_json::Value = serde_json::from_str(raw)?;
    match value.get("schema") {
        None => {
            // The version-zero writer only supported level-one Wizards.
            ensure!(
                value.get("class").is_none() && value.get("base").is_none(),
                "class metadata requires a schema version"
            );
            ensure!(
                value["level"] == 1 && value["exp"] == 0,
                "unrecognized legacy character progression"
            );
            value["schema"] = 1.into();
            value["class"] = WIZARD.into();
            value["base"] = serde_json::to_value(legacy_base)?;
        }
        Some(schema) => ensure!(schema == 1, "unsupported character schema {schema}"),
    }
    Ok(serde_json::from_value(value)?)
}
impl Store {
    pub fn open(path: &Path, content: &Content) -> Result<Self> {
        let legacy_base = content
            .classes
            .get(&WIZARD)
            .context("Wizard class missing")?
            .base;
        let mut c = Connection::open(path)?;
        c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
CREATE TABLE IF NOT EXISTS accounts(id INTEGER PRIMARY KEY,guid TEXT NOT NULL UNIQUE,name TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS shore_bots(name TEXT PRIMARY KEY,account INTEGER NOT NULL UNIQUE REFERENCES accounts(id));
CREATE TABLE IF NOT EXISTS characters(id INTEGER PRIMARY KEY,account INTEGER NOT NULL REFERENCES accounts(id),stats TEXT NOT NULL,items TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 0,alive INTEGER NOT NULL DEFAULT 1);
CREATE UNIQUE INDEX IF NOT EXISTS one_living_character ON characters(account) WHERE alive=1;
CREATE TABLE IF NOT EXISTS deaths(character INTEGER PRIMARY KEY REFERENCES characters(id),snapshot TEXT NOT NULL,killed_by TEXT NOT NULL,died_at INTEGER NOT NULL DEFAULT(unixepoch()));
CREATE TABLE IF NOT EXISTS vaults(account INTEGER PRIMARY KEY REFERENCES accounts(id),items TEXT NOT NULL CHECK(json_array_length(items)=8),revision INTEGER NOT NULL DEFAULT 0);")?;
        let migration = c.transaction()?;
        let records = {
            let mut q = migration.prepare("SELECT id,stats FROM characters ORDER BY id")?;
            q.query_map([], |r| Ok((r.get::<_, i32>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut migrated = 0;
        for (id, raw) in records {
            let ch = decode_character(&raw, legacy_base)
                .with_context(|| format!("character {id} migration"))?;
            let definition = content
                .classes
                .get(&ch.class)
                .context("persisted character has unknown class")?;
            ensure!(
                (1..=20).contains(&ch.level)
                    && ch
                        .base
                        .iter()
                        .zip(definition.caps)
                        .all(|(v, cap)| *v >= 0 && *v <= cap),
                "invalid persisted class stats for character {id}"
            );
            if serde_json::from_str::<serde_json::Value>(&raw)?
                .get("schema")
                .is_none()
            {
                migration.execute(
                    "UPDATE characters SET stats=?2 WHERE id=?1",
                    params![id, serde_json::to_string(&ch)?],
                )?;
                migrated += 1;
            }
        }
        migration.commit()?;
        if migrated > 0 {
            tracing::info!(
                migrated,
                from_schema = 0,
                to_schema = 1,
                "character metadata migrated; death snapshots unchanged"
            );
        }
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("account-store".into())
            .spawn(move || {
                let mut c = c;
                for job in rx {
                    job(&mut c);
                }
            })?;
        Ok(Self { tx, legacy_base })
    }
    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Box::new(move |c| {
                let _ = tx.send(f(c));
            }))
            .map_err(|_| anyhow::anyhow!("account store stopped"))?;
        rx.await.context("account worker failed")?
    }
    pub async fn list(&self, guid: String) -> Result<Account> {
        ensure!(
            !guid.is_empty()
                && guid.len() <= 128
                && guid
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.@".contains(c)),
            "invalid guest identity"
        );
        let account = self.call(move|c|{let tx=c.transaction()?;let created=tx.execute("INSERT OR IGNORE INTO accounts(guid,name) VALUES(?1,'Guest')",[&guid])?;
            let (id,mut name):(i32,String)=tx.query_row("SELECT id,name FROM accounts WHERE guid=?1",[&guid],|r|Ok((r.get(0)?,r.get(1)?)))?;
            if created==1{name=format!("Guest{id}");tx.execute("UPDATE accounts SET name=?2 WHERE id=?1",params![id,name])?;}
            let mut characters=Vec::new();{let mut q=tx.prepare("SELECT id,stats,items,revision,alive FROM characters WHERE account=?1 AND alive=1 ORDER BY id")?;let rows=q.query_map([id],|r|Ok((r.get::<_,i32>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i32>(3)?,r.get::<_,bool>(4)?)))?;
            for row in rows{let(cid,stats,items,rev,alive)=row?;let mut ch:Character=serde_json::from_str(&stats)?;ch.id=cid;ch.account=id;ch.name=name.clone();ch.items=serde_json::from_str(&items)?;ch.revision=rev;ch.alive=alive;characters.push(ch);}}
            let next_id:i32=tx.query_row("SELECT coalesce(max(id),0)+1 FROM characters WHERE account=?1",[id],|r|r.get(0))?;let best_levels = { let mut q = tx.prepare("SELECT json_extract(stats,'$.class'),max(json_extract(stats,'$.level')) FROM characters WHERE account=?1 GROUP BY json_extract(stats,'$.class')")?; q.query_map([id], |r| Ok((r.get::<_,u16>(0)?, r.get::<_,i32>(1)?)))?.collect::<rusqlite::Result<std::collections::BTreeMap<_,_>>>()? };tx.commit()?;Ok(Account{id,name,next_id,best_levels,characters})}).await?;
        self.vault(account.id).await?;
        Ok(account)
    }
    pub async fn shore_character(
        &self,
        name: String,
        wizard: &Object,
    ) -> Result<Option<Character>> {
        let guid = self.call(move |db| {
            let tx = db.transaction()?;
            let existing: Option<String> = tx.query_row(
                "SELECT guid FROM accounts JOIN shore_bots ON accounts.id=shore_bots.account WHERE shore_bots.name=?1",
                [&name], |r| r.get(0)).optional()?;
            let guid = match existing {
                Some(guid) => guid,
                None => {
                    let guid = format!("shore-{:032x}", rand::random::<u128>());
                    tx.execute("INSERT INTO accounts(guid,name) VALUES(?1,?2)", params![guid,name])?;
                    tx.execute("INSERT INTO shore_bots(name,account) VALUES(?1,?2)", params![name,tx.last_insert_rowid()])?;
                    guid
                }
            };
            tx.commit()?;
            Ok(guid)
        }).await?;
        let mut account = self.list(guid).await?;
        ensure!(
            account.characters.len() <= 1,
            "shore account has multiple live characters"
        );
        if account.next_id == 1 {
            Ok(Some(self.create(account.id, wizard).await?))
        } else {
            Ok(account.characters.pop())
        }
    }
    pub async fn authenticate(&self, guid: String) -> Result<i32> {
        self.call(move |c| {
            c.query_row("SELECT id FROM accounts WHERE guid=?1", [guid], |r| {
                r.get(0)
            })
            .context("unknown account; load character list first")
        })
        .await
    }
    pub async fn create(&self, account: i32, wizard: &Object) -> Result<Character> {
        ensure!(
            wizard.kind == WIZARD && wizard.class == "Player",
            "playable class is not implemented"
        );
        let base = [
            wizard.hp,
            wizard.mp,
            wizard.attack,
            wizard.defense,
            wizard.speed,
            wizard.dexterity,
            wizard.vitality,
            wizard.wisdom,
        ];
        let hp = wizard.hp;
        let mp = wizard.mp;
        let items = wizard.equipment.clone();
        self.call(move |c| {
            let tx = c.transaction()?;
            let name: String =
                tx.query_row("SELECT name FROM accounts WHERE id=?1", [account], |r| {
                    r.get(0)
                })?;
            let mut ch = Character {
                schema: 1,
                class: WIZARD,
                base,
                id: 0,
                account,
                name,
                level: 1,
                exp: 0,
                fame: 0,
                hp,
                mp,
                items,
                revision: 0,
                alive: true,
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
                shots: 0,
                hits: 0,
                kills: 0,
                quests: 0,
                tiles: 0,
                pirate_caves: 0,
            };
            tx.execute(
                "INSERT INTO characters(account,stats,items) VALUES(?1,?2,?3)",
                params![
                    account,
                    serde_json::to_string(&ch)?,
                    serde_json::to_string(&ch.items)?
                ],
            )?;
            ch.id = i32::try_from(tx.last_insert_rowid())?;
            tx.commit()?;
            Ok(ch)
        })
        .await
    }
    pub async fn load(&self, account: i32, id: i32) -> Result<Character> {
        self.call(move|c|{let(stats,items,revision):(String,String,i32)=c.query_row("SELECT stats,items,revision FROM characters WHERE id=?1 AND account=?2 AND alive=1",params![id,account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).context("character does not belong to this account or is dead")?;let mut ch:Character=serde_json::from_str(&stats)?;ch.id=id;ch.account=account;ch.items=serde_json::from_str(&items)?;ch.revision=revision;ch.alive=true;Ok(ch)}).await
    }
    pub async fn vault(&self, account: i32) -> Result<Vault> {
        self.call(move |c| {
            let tx = c.transaction()?;
            tx.execute(
                "INSERT OR IGNORE INTO vaults(account,items) VALUES(?1,?2)",
                params![account, serde_json::to_string(&vec![-1; 8])?],
            )?;
            let (items, revision): (String, i32) = tx.query_row(
                "SELECT items,revision FROM vaults WHERE account=?1",
                [account],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            tx.commit()?;
            Ok(Vault {
                account,
                items: serde_json::from_str(&items)?,
                revision,
            })
        })
        .await
    }
    pub async fn exchange(
        &self,
        writes: Vec<InventoryWrite>,
        vault: Option<Vault>,
    ) -> Result<Vec<i32>> {
        ensure!(
            (1..=2).contains(&writes.len()),
            "exchange must contain one or two characters"
        );
        if let Some(vault) = &vault {
            ensure!(
                writes.len() == 1 && writes[0].account == vault.account,
                "vault exchange must use its owner's character"
            );
            ensure!(
                vault.items.len() == 8 && vault.items.iter().all(|i| (-1..=65535).contains(i)),
                "invalid eight-slot vault"
            );
            ensure!(
                vault.revision >= 0 && vault.revision < i32::MAX,
                "invalid vault revision"
            );
        }
        let mut owners = std::collections::BTreeSet::new();
        for write in &writes {
            ensure!(owners.insert(write.account), "exchange repeats an account");
            ensure!(
                write.items.len() == 12 && write.items.iter().all(|i| (-1..=65535).contains(i)),
                "invalid twelve-slot inventory"
            );
            ensure!(
                write.revision >= 0 && write.revision < i32::MAX,
                "invalid inventory revision"
            );
        }
        self.call(move |c| {
            let tx = c.transaction()?;
            let mut revisions = Vec::new();
            for write in writes {
                let n = tx.execute("UPDATE characters SET items=?4,revision=revision+1 WHERE id=?1 AND account=?2 AND revision=?3 AND alive=1", params![write.id, write.account, write.revision, serde_json::to_string(&write.items)?])?;
                ensure!(n == 1, "inventory conflict: character {}, account {}, expected revision {}", write.id, write.account, write.revision);
                revisions.push(write.revision + 1);
            }
            if let Some(vault) = vault {
                let n = tx.execute("UPDATE vaults SET items=?3,revision=revision+1 WHERE account=?1 AND revision=?2", params![vault.account, vault.revision, serde_json::to_string(&vault.items)?])?;
                ensure!(n == 1, "vault conflict: account {}, expected revision {}", vault.account, vault.revision);
            }
            tx.commit()?;
            Ok(revisions)
        }).await
    }
    pub async fn progress(&self, ch: Character) -> Result<()> {
        self.progress_batch(vec![ch]).await
    }
    pub async fn progress_batch(&self, characters: Vec<Character>) -> Result<()> {
        ensure!(
            (1..=85).contains(&characters.len()),
            "invalid progress batch size"
        );
        let mut accounts = std::collections::BTreeSet::new();
        ensure!(
            characters.iter().all(|ch| accounts.insert(ch.account)),
            "progress batch repeats an account"
        );
        self.call(move |c| {
            let tx = c.transaction()?;
            for ch in characters {
                let n = tx.execute("UPDATE characters SET stats=?3 WHERE id=?1 AND account=?2 AND alive=1 AND revision=?4", params![ch.id, ch.account, serde_json::to_string(&ch)?, ch.revision])?;
                ensure!(n == 1, "progress conflict: dead, missing or stale character {} at revision {}", ch.id, ch.revision);
            }
            tx.commit()?;
            Ok(())
        }).await
    }
    pub async fn death(&self, ch: Character, killed_by: String) -> Result<()> {
        ensure!(!ch.alive && ch.hp == 0, "death snapshot must be dead");
        self.call(move|c|{let tx=c.transaction()?;let n=tx.execute("UPDATE characters SET alive=0,stats=?3,items=?4,revision=revision+1 WHERE id=?1 AND account=?2 AND alive=1 AND revision=?5",params![ch.id,ch.account,serde_json::to_string(&ch)?,serde_json::to_string(&vec![-1;12])?,ch.revision])?;ensure!(n==1,"death already recorded or inventory conflict");tx.execute("INSERT INTO deaths(character,snapshot,killed_by) VALUES(?1,?2,?3)",params![ch.id,serde_json::to_string(&ch)?,killed_by])?;tx.commit()?;Ok(())}).await
    }
    pub async fn fame(&self, account: i32, id: i32) -> Result<String> {
        let legacy_base = self.legacy_base;
        self.call(move|db|{let (snapshot,killer):(String,String)=db.query_row("SELECT snapshot,killed_by FROM deaths JOIN characters ON characters.id=deaths.character WHERE characters.account=?1 AND deaths.character=?2",params![account,id],|r|Ok((r.get(0)?,r.get(1)?)))?;let c = decode_character(&snapshot, legacy_base)?;ensure!(c.created_at>0,"creation time was not recorded by the early development binary for this character");Ok(format!("<Fame><Char id=\"{}\"><Account><Name>{}</Name></Account><ObjectType>{}</ObjectType><Level>{}</Level><Tex1>0</Tex1><Tex2>0</Tex2></Char><CreatedOn>{}</CreatedOn><KilledBy>{}</KilledBy><TotalFame>{}</TotalFame><Shots>{}</Shots><ShotsThatDamage>{}</ShotsThatDamage><TilesUncovered>{}</TilesUncovered><MonsterKills>{}</MonsterKills><GodKills>0</GodKills><OryxKills>0</OryxKills><QuestsCompleted>{}</QuestsCompleted><PirateCavesCompleted>{}</PirateCavesCompleted><UndeadLairsCompleted>0</UndeadLairsCompleted><AbyssOfDemonsCompleted>0</AbyssOfDemonsCompleted><SnakePitsCompleted>0</SnakePitsCompleted><SpiderDensCompleted>0</SpiderDensCompleted><SpriteWorldsCompleted>0</SpriteWorldsCompleted><TombsCompleted>0</TombsCompleted><LevelUpAssists>0</LevelUpAssists><BaseFame>{}</BaseFame></Fame>",c.id,xml_escape(&c.name),c.class,c.level,c.created_at,xml_escape(&killer),c.fame,c.shots,c.hits,c.tiles,c.kills,c.quests,c.pirate_caves,c.fame))}).await
    }
    pub async fn rename(&self, account: i32, name: String) -> Result<()> {
        ensure!(
            (3..=10).contains(&name.len()) && name.bytes().all(|c| c.is_ascii_alphabetic()),
            "names must be 3–10 letters"
        );
        self.call(move |c| {
            let n = c.execute(
                "UPDATE accounts SET name=?2 WHERE id=?1",
                params![account, name],
            )?;
            ensure!(n == 1, "account missing");
            Ok(())
        })
        .await
    }
}
pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
impl Account {
    pub fn xml(&self) -> String {
        let mut chars = String::new();
        for c in &self.characters {
            chars += &format!(
                "<Char id=\"{}\"><ObjectType>{}</ObjectType><Level>{}</Level><Exp>{}</Exp><CurrentFame>{}</CurrentFame><Tex1>0</Tex1><Tex2>0</Tex2><Equipment>{}</Equipment><MaxHitPoints>{}</MaxHitPoints><HitPoints>{}</HitPoints><MaxMagicPoints>{}</MaxMagicPoints><MagicPoints>{}</MagicPoints><Attack>{}</Attack><Defense>{}</Defense><Speed>{}</Speed><Dexterity>{}</Dexterity><HpRegen>{}</HpRegen><MpRegen>{}</MpRegen></Char>",
                c.id,
                c.class,
                c.level,
                c.exp,
                c.fame,
                c.items
                    .iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
                c.base[0],
                c.hp,
                c.base[1],
                c.mp,
                c.base[2],
                c.base[3],
                c.base[4],
                c.base[5],
                c.base[6],
                c.base[7]
            );
        }
        let records: String = self.best_levels.iter().map(|(class, level)| format!("<ClassStats objectType=\"{class}\"><BestLevel>{level}</BestLevel><BestFame>0</BestFame></ClassStats>")).collect();
        format!(
            "<Chars nextCharId=\"{}\" maxNumChars=\"1\">{chars}<Account><AccountId>{}</AccountId><Name>{}</Name><NameChosen/><Stats><Fame>0</Fame><TotalFame>0</TotalFame>{records}</Stats><Credits>0</Credits><NextCharSlotPrice>600</NextCharSlotPrice></Account><Servers><Server><Name>Local development</Name><DNS>127.0.0.1</DNS><Lat>0</Lat><Long>0</Long><Usage>0</Usage></Server></Servers><News/></Chars>",
            self.next_id,
            self.id,
            xml_escape(&self.name)
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cooperative_progress_batch_rolls_back_every_stale_participant() {
        let content = Content::load(Path::new("data")).unwrap();
        let s = Store::open(Path::new(":memory:"), &content).unwrap();
        let mut characters = Vec::new();
        for guid in ["progress-a", "progress-b"] {
            let account = s.list(guid.into()).await.unwrap();
            let mut ch = s
                .create(account.id, content.get(WIZARD).unwrap())
                .await
                .unwrap();
            ch.exp = 49;
            ch.pirate_caves = 1;
            characters.push(ch);
        }
        characters[1].revision = 1;
        assert!(s.progress_batch(characters.clone()).await.is_err());
        for ch in &characters {
            let saved = s.load(ch.account, ch.id).await.unwrap();
            assert_eq!(saved.exp, 0);
            assert_eq!(saved.pirate_caves, 0);
        }
        characters[1].revision = 0;
        s.progress_batch(characters.clone()).await.unwrap();
        for ch in &characters {
            let mut saved = s.load(ch.account, ch.id).await.unwrap();
            assert_eq!(saved.exp, 49);
            assert_eq!(saved.pirate_caves, 1);
            saved.alive = false;
            saved.hp = 0;
            saved.items.fill(-1);
            s.death(saved, "test clear snapshot".into()).await.unwrap();
            assert!(
                s.fame(ch.account, ch.id)
                    .await
                    .unwrap()
                    .contains("<PirateCavesCompleted>1</PirateCavesCompleted>")
            );
        }
    }
    #[tokio::test]
    async fn experience_thresholds_growth_caps_and_level_twenty_boundary() {
        use rand::SeedableRng;
        let content = Content::load(Path::new("data")).unwrap();
        let s = Store::open(Path::new(":memory:"), &content).unwrap();
        let account = s.list("experience".into()).await.unwrap();
        let mut ch = s
            .create(account.id, content.get(WIZARD).unwrap())
            .await
            .unwrap();
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        assert_eq!(level_start(20).unwrap(), 18050);
        assert_eq!(experience_goal(20).unwrap(), 1950);
        assert!(level_start(0).is_err());
        assert!(!ch.gain_experience(&content, 49, &mut rng).unwrap());
        ch.hp = 1;
        ch.mp = 1;
        assert!(ch.gain_experience(&content, 1, &mut rng).unwrap());
        assert_eq!(ch.level, 2);
        for (i, (min, max)) in content.classes[&WIZARD].growth.iter().enumerate() {
            let increase = ch.base[i] - content.classes[&WIZARD].base[i];
            assert!((*min..=*max).contains(&increase));
        }
        assert_eq!(ch.hp, ch.effective(&content).unwrap()[0]);
        assert_eq!(ch.mp, ch.effective(&content).unwrap()[1]);
        assert!(ch.gain_experience(&content, 150, &mut rng).unwrap());
        assert_eq!(ch.level, 3);
        let amount = 18050 - ch.exp;
        assert!(ch.gain_experience(&content, amount, &mut rng).unwrap());
        assert_eq!(ch.level, 20);
        assert!(
            ch.base
                .iter()
                .zip(content.classes[&WIZARD].caps)
                .all(|(v, cap)| *v <= cap)
        );
        let base = ch.base;
        assert!(!ch.gain_experience(&content, 195, &mut rng).unwrap());
        assert_eq!(ch.base, base);
        assert_eq!(ch.exp, 18245);
        assert!(ch.gain_experience(&content, -1, &mut rng).is_err());
        assert!(ch.gain_experience(&content, i32::MAX, &mut rng).is_err());
        assert_eq!(ch.exp, 18245);
        s.progress(ch.clone()).await.unwrap();
        let loaded = s.load(ch.account, ch.id).await.unwrap();
        assert_eq!(loaded.level, 20);
        assert_eq!(loaded.exp, ch.exp);
        assert_eq!(loaded.base, ch.base);
    }
    #[tokio::test]
    async fn vault_exchange_is_atomic_private_and_survives_death() {
        let content = Content::load(Path::new("data")).unwrap();
        let s = Store::open(Path::new(":memory:"), &content).unwrap();
        let owner = s.list("vault-owner".into()).await.unwrap();
        let other = s.list("vault-other".into()).await.unwrap();
        let mut ch = s
            .create(owner.id, content.get(WIZARD).unwrap())
            .await
            .unwrap();
        let mut chest = s.vault(owner.id).await.unwrap();
        assert_eq!(chest.items, vec![-1; 8]);
        let original = ch.items.clone();
        chest.items[0] = ch.items[4];
        let mut write = InventoryWrite {
            id: ch.id,
            account: ch.account,
            revision: 0,
            items: ch.items.clone(),
        };
        write.items[4] = -1;
        chest.revision = 1;
        assert!(
            s.exchange(vec![write.clone()], Some(chest.clone()))
                .await
                .is_err()
        );
        assert_eq!(s.load(owner.id, ch.id).await.unwrap().items, original);
        assert_eq!(s.vault(owner.id).await.unwrap().items, vec![-1; 8]);
        chest.revision = 0;
        let mut foreign = chest.clone();
        foreign.account = other.id;
        assert!(
            s.exchange(vec![write.clone()], Some(foreign))
                .await
                .is_err()
        );
        assert_eq!(
            s.exchange(vec![write], Some(chest.clone())).await.unwrap(),
            vec![1]
        );
        ch = s.load(owner.id, ch.id).await.unwrap();
        assert_eq!(ch.items[4], -1);
        assert_eq!(s.vault(owner.id).await.unwrap().items[0], original[4]);
        assert_eq!(s.vault(other.id).await.unwrap().items, vec![-1; 8]);
        ch.hp = 0;
        ch.alive = false;
        let dead_id = ch.id;
        s.death(ch, "vault test".into()).await.unwrap();
        let replacement = s
            .create(owner.id, content.get(WIZARD).unwrap())
            .await
            .unwrap();
        assert_ne!(replacement.id, dead_id);
        assert_eq!(s.vault(owner.id).await.unwrap().items[0], original[4]);
        let mut invalid = s.vault(owner.id).await.unwrap();
        invalid.items.push(-1);
        let write = InventoryWrite {
            id: replacement.id,
            account: owner.id,
            revision: 0,
            items: replacement.items,
        };
        assert!(s.exchange(vec![write], Some(invalid)).await.is_err());
    }
    #[tokio::test]
    async fn qualifying_class_records_survive_death_and_match_unlock_requirements() {
        let content = Content::load(Path::new("data")).unwrap();
        let s = Store::open(Path::new(":memory:"), &content).unwrap();
        let account = s.list("class-records".into()).await.unwrap();
        assert!(content.unlocked(WIZARD, &account.best_levels).unwrap());
        assert!(!content.unlocked(0x0310, &account.best_levels).unwrap());
        let mut ch = s
            .create(account.id, content.get(WIZARD).unwrap())
            .await
            .unwrap();
        ch.level = 5;
        ch.exp = 800;
        s.progress(ch.clone()).await.unwrap();
        ch.hp = 0;
        ch.alive = false;
        s.death(ch, "record test".into()).await.unwrap();
        let account = s.list("class-records".into()).await.unwrap();
        assert_eq!(account.best_levels.get(&WIZARD), Some(&5));
        assert!(account.characters.is_empty());
        assert!(content.unlocked(0x0310, &account.best_levels).unwrap());
        assert!(!content.unlocked(0x0300, &account.best_levels).unwrap());
        assert!(!content.unlocked(0x031f, &account.best_levels).unwrap());
        let xml = account.xml();
        let parsed = roxmltree::Document::parse(&xml).unwrap();
        let record = parsed
            .descendants()
            .find(|n| n.has_tag_name("ClassStats"))
            .unwrap();
        assert_eq!(record.attribute("objectType"), Some("782"));
        assert_eq!(
            record
                .children()
                .find(|n| n.has_tag_name("BestLevel"))
                .unwrap()
                .text(),
            Some("5")
        );
    }
    #[tokio::test]
    async fn equipment_derives_stats_without_stacking_or_backpack_bonuses() {
        let content = Content::load(Path::new("data")).unwrap();
        let s = Store::open(Path::new(":memory:"), &content).unwrap();
        let account = s.list("effective-stats".into()).await.unwrap();
        let mut ch = s
            .create(account.id, content.get(WIZARD).unwrap())
            .await
            .unwrap();
        assert_eq!(ch.effective(&content).unwrap(), ch.base);
        ch.items[3] = 0xa4f;
        let equipped = ch.effective(&content).unwrap();
        assert_eq!(equipped[3], ch.base[3] + 1);
        assert_eq!(ch.effective(&content).unwrap(), equipped);
        ch.items[3] = -1;
        ch.items[4] = 0xa4f;
        assert_eq!(ch.effective(&content).unwrap(), ch.base);
        ch.items[3] = ch.items[0];
        assert!(ch.effective(&content).is_err());
        ch.items[3] = -1;
        ch.base[0] = content.classes[&WIZARD].caps[0] + 1;
        assert!(ch.effective(&content).is_err());
    }
    #[tokio::test]
    async fn shore_accounts_resume_committed_characters_and_never_rebuild_the_dead() {
        let content = Content::load(Path::new("data")).unwrap();
        let path = std::env::temp_dir().join(format!(
            "openrotmg-shore-{}-{}.db",
            std::process::id(),
            rand::random::<u64>()
        ));
        let store = Store::open(&path, &content).unwrap();
        let wizard = content.get(WIZARD).unwrap();
        let human = store.list("shore-human".into()).await.unwrap();
        store.rename(human.id, "Bayleaf".into()).await.unwrap();
        let mut bot = store
            .shore_character("Bayleaf".into(), wizard)
            .await
            .unwrap()
            .unwrap();
        assert_ne!(bot.account, human.id);
        let identity = (bot.account, bot.id);
        assert_eq!(bot.items, wizard.equipment);
        bot.exp = 12;
        bot.shots = 4;
        bot.hp = 42;
        store.progress(bot.clone()).await.unwrap();
        let reopened = Store::open(&path, &content).unwrap();
        let resumed = reopened
            .shore_character("Bayleaf".into(), wizard)
            .await
            .unwrap()
            .unwrap();
        assert_eq!((resumed.account, resumed.id), identity);
        assert_eq!(resumed.exp, 12);
        assert_eq!(resumed.hp, 42);
        assert_eq!(resumed.shots, 4);
        bot.hp = 0;
        bot.alive = false;
        store.death(bot, "test enemy".into()).await.unwrap();
        assert!(
            reopened
                .shore_character("Bayleaf".into(), wizard)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .shore_character("Bayleaf".into(), wizard)
                .await
                .unwrap()
                .is_none()
        );
        let records: i32 = store
            .call(move |db| {
                Ok(db.query_row(
                    "SELECT count(*) FROM characters WHERE account=?1",
                    [identity.0],
                    |r| r.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(records, 1);
        drop(reopened);
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn legacy_metadata_migration_preserves_immutable_death_bytes() {
        let content = Content::load(Path::new("data")).unwrap();
        let path = std::env::temp_dir().join(format!(
            "openrotmg-migration-{}-{}.db",
            std::process::id(),
            rand::random::<u64>()
        ));
        let s = Store::open(&path, &content).unwrap();
        let mut characters = Vec::new();
        for guid in ["legacy-live", "legacy-dead"] {
            let account = s.list(guid.into()).await.unwrap();
            characters.push(
                s.create(account.id, content.get(WIZARD).unwrap())
                    .await
                    .unwrap(),
            );
        }
        let mut dead = characters[1].clone();
        dead.hp = 0;
        dead.alive = false;
        s.death(dead.clone(), "legacy pirate".into()).await.unwrap();
        let legacy = |ch: &Character| {
            let mut value = serde_json::to_value(ch).unwrap();
            for key in ["schema", "class", "base", "pirate_caves"] {
                value.as_object_mut().unwrap().remove(key);
            }
            serde_json::to_string(&value).unwrap()
        };
        let live_raw = legacy(&characters[0]);
        let dead_raw = legacy(&dead);
        let expected = dead_raw.clone();
        let live_id = characters[0].id;
        let dead_id = dead.id;
        s.call(move |db| {
            db.execute(
                "UPDATE characters SET stats=?2 WHERE id=?1",
                params![live_id, live_raw],
            )?;
            db.execute(
                "UPDATE characters SET stats=?2 WHERE id=?1",
                params![dead_id, dead_raw],
            )?;
            db.execute(
                "UPDATE deaths SET snapshot=?2 WHERE character=?1",
                params![dead_id, dead_raw],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        drop(s);
        let s = Store::open(&path, &content).unwrap();
        let migrated = s.load(characters[0].account, live_id).await.unwrap();
        assert_eq!(migrated.schema, 1);
        assert_eq!(migrated.class, WIZARD);
        assert_eq!(migrated.base, content.classes[&WIZARD].base);
        assert!(
            s.fame(dead.account, dead.id)
                .await
                .unwrap()
                .contains("legacy pirate")
        );
        let unchanged: String = s
            .call(move |db| {
                Ok(db.query_row(
                    "SELECT snapshot FROM deaths WHERE character=?1",
                    [dead_id],
                    |r| r.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(unchanged, expected);
        assert!(s.load(dead.account, dead.id).await.is_err());
        assert!(decode_character("{\"schema\":2}", migrated.base).is_err());
        assert!(decode_character("{\"level\":1,\"exp\":0,\"class\":784}", migrated.base).is_err());
        assert!(decode_character("{\"level\":2,\"exp\":50}", migrated.base).is_err());
        drop(s);
        std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn exchange_is_atomic_and_rejects_stale_or_foreign_owners() {
        let content = crate::content::Content::load(Path::new("data")).unwrap();
        let s = Store::open(Path::new(":memory:"), &content).unwrap();
        let mut characters = Vec::new();
        for guid in ["exchange-a", "exchange-b"] {
            let account = s.list(guid.into()).await.unwrap();
            characters.push(
                s.create(account.id, content.get(crate::content::WIZARD).unwrap())
                    .await
                    .unwrap(),
            );
        }
        let mut writes: Vec<_> = characters
            .iter()
            .map(|ch| InventoryWrite {
                id: ch.id,
                account: ch.account,
                revision: ch.revision,
                items: ch.items.clone(),
            })
            .collect();
        writes[0].items[4] = -1;
        writes[1].items[5] = characters[0].items[4];
        writes[1].revision = 1;
        assert!(s.exchange(writes.clone(), None).await.is_err());
        for ch in &characters {
            let loaded = s.load(ch.account, ch.id).await.unwrap();
            assert_eq!(loaded.items, ch.items);
            assert_eq!(loaded.revision, 0);
        }
        writes[1].revision = 0;
        let mut foreign = writes.clone();
        foreign[1].id = characters[0].id;
        assert!(s.exchange(foreign, None).await.is_err());
        assert_eq!(s.exchange(writes.clone(), None).await.unwrap(), vec![1, 1]);
        for (ch, write) in characters.iter().zip(&writes) {
            let loaded = s.load(ch.account, ch.id).await.unwrap();
            assert_eq!(loaded.items, write.items);
            assert_eq!(loaded.revision, 1);
        }
        assert!(s.exchange(writes, None).await.is_err());
        assert!(s.progress(characters[0].clone()).await.is_err());
        let mut current = s
            .load(characters[0].account, characters[0].id)
            .await
            .unwrap();
        current.exp = 49;
        s.progress(current.clone()).await.unwrap();
        assert_eq!(s.load(current.account, current.id).await.unwrap().exp, 49);
    }
    #[tokio::test]
    async fn inventory_is_versioned_and_death_is_permanent() {
        let c = crate::content::Content::load(Path::new("data")).unwrap();
        let s = Store::open(Path::new(":memory:"), &c).unwrap();
        let a = s.list("test-account".into()).await.unwrap();
        let mut ch = s
            .create(a.id, c.get(crate::content::WIZARD).unwrap())
            .await
            .unwrap();
        let mut inv = ch.items.clone();
        inv[4] = -1;
        let write = InventoryWrite {
            id: ch.id,
            account: a.id,
            revision: 0,
            items: inv.clone(),
        };
        ch.revision = s.exchange(vec![write.clone()], None).await.unwrap()[0];
        assert!(s.exchange(vec![write], None).await.is_err());
        ch.items = inv;
        ch.hp = 0;
        ch.alive = false;
        s.death(ch.clone(), "test enemy".into()).await.unwrap();
        assert!(s.load(a.id, ch.id).await.is_err());
        assert!(s.death(ch.clone(), "test enemy".into()).await.is_err());
        let fame = s.fame(a.id, ch.id).await.unwrap();
        let root = roxmltree::Document::parse(&fame).unwrap();
        assert_eq!(root.root_element().tag_name().name(), "Fame");
        assert_eq!(
            root.descendants()
                .find(|n| n.has_tag_name("KilledBy"))
                .unwrap()
                .text(),
            Some("test enemy")
        );
        let fresh = s.list("second-account".into()).await.unwrap();
        assert_eq!(fresh.next_id, 1);
        s.rename(fresh.id, "Guest".into()).await.unwrap();
        assert_eq!(s.list("second-account".into()).await.unwrap().name, "Guest");
        assert!(
            s.list("test-account".into())
                .await
                .unwrap()
                .characters
                .is_empty()
        );
    }
}
