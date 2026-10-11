use crate::content::Object;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::mpsc};
use tokio::sync::oneshot;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Character {
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
}
#[derive(Clone, Debug, Serialize)]
pub struct Account {
    pub id: i32,
    pub name: String,
    pub next_id: i32,
    pub best_level: i32,
    pub characters: Vec<Character>,
}
type Job = Box<dyn FnOnce(&mut Connection) + Send>;
#[derive(Clone)]
pub struct Store {
    tx: mpsc::Sender<Job>,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let c = Connection::open(path)?;
        c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
CREATE TABLE IF NOT EXISTS accounts(id INTEGER PRIMARY KEY,guid TEXT NOT NULL UNIQUE,name TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS characters(id INTEGER PRIMARY KEY,account INTEGER NOT NULL REFERENCES accounts(id),stats TEXT NOT NULL,items TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 0,alive INTEGER NOT NULL DEFAULT 1);
CREATE UNIQUE INDEX IF NOT EXISTS one_living_character ON characters(account) WHERE alive=1;
CREATE TABLE IF NOT EXISTS deaths(character INTEGER PRIMARY KEY REFERENCES characters(id),snapshot TEXT NOT NULL,killed_by TEXT NOT NULL,died_at INTEGER NOT NULL DEFAULT(unixepoch()));")?;
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("account-store".into())
            .spawn(move || {
                let mut c = c;
                for job in rx {
                    job(&mut c);
                }
            })?;
        Ok(Self { tx })
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
        self.call(move|c|{let tx=c.transaction()?;let created=tx.execute("INSERT OR IGNORE INTO accounts(guid,name) VALUES(?1,'Guest')",[&guid])?;
            let (id,mut name):(i32,String)=tx.query_row("SELECT id,name FROM accounts WHERE guid=?1",[&guid],|r|Ok((r.get(0)?,r.get(1)?)))?;
            if created==1{name=format!("Guest{id}");tx.execute("UPDATE accounts SET name=?2 WHERE id=?1",params![id,name])?;}
            let mut characters=Vec::new();{let mut q=tx.prepare("SELECT id,stats,items,revision,alive FROM characters WHERE account=?1 AND alive=1 ORDER BY id")?;let rows=q.query_map([id],|r|Ok((r.get::<_,i32>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i32>(3)?,r.get::<_,bool>(4)?)))?;
            for row in rows{let(cid,stats,items,rev,alive)=row?;let mut ch:Character=serde_json::from_str(&stats)?;ch.id=cid;ch.account=id;ch.name=name.clone();ch.items=serde_json::from_str(&items)?;ch.revision=rev;ch.alive=alive;characters.push(ch);}}
            let next_id:i32=tx.query_row("SELECT coalesce(max(id),0)+1 FROM characters WHERE account=?1",[id],|r|r.get(0))?;let best_level:i32=tx.query_row("SELECT coalesce(max(json_extract(stats,'$.level')),0) FROM characters WHERE account=?1",[id],|r|r.get(0))?;tx.commit()?;Ok(Account{id,name,next_id,best_level,characters})}).await
    }
    pub async fn authenticate(&self, guid: String) -> Result<i32> {
        self.call(move |c| {
            Ok(
                c.query_row("SELECT id FROM accounts WHERE guid=?1", [guid], |r| {
                    r.get(0)
                })
                .context("unknown account; load character list first")?,
            )
        })
        .await
    }
    pub async fn create(&self, account: i32, wizard: &Object) -> Result<Character> {
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
    pub async fn inventory(
        &self,
        id: i32,
        account: i32,
        revision: i32,
        items: Vec<i32>,
    ) -> Result<i32> {
        ensure!(items.len() == 12, "inventory must have twelve slots");
        self.call(move|c|{let tx=c.transaction()?;let n=tx.execute("UPDATE characters SET items=?4,revision=revision+1 WHERE id=?1 AND account=?2 AND revision=?3 AND alive=1",params![id,account,revision,serde_json::to_string(&items)?])?;ensure!(n==1,"inventory conflict or dead character");tx.commit()?;Ok(revision+1)}).await
    }
    pub async fn progress(&self, ch: Character) -> Result<()> {
        self.call(move |c| {
            let n = c.execute(
                "UPDATE characters SET stats=?3 WHERE id=?1 AND account=?2 AND alive=1",
                params![ch.id, ch.account, serde_json::to_string(&ch)?],
            )?;
            ensure!(n == 1, "cannot save a dead or missing character");
            Ok(())
        })
        .await
    }
    pub async fn death(&self, ch: Character, killed_by: String) -> Result<()> {
        ensure!(!ch.alive && ch.hp == 0, "death snapshot must be dead");
        self.call(move|c|{let tx=c.transaction()?;let n=tx.execute("UPDATE characters SET alive=0,stats=?3,items=?4,revision=revision+1 WHERE id=?1 AND account=?2 AND alive=1 AND revision=?5",params![ch.id,ch.account,serde_json::to_string(&ch)?,serde_json::to_string(&vec![-1;12])?,ch.revision])?;ensure!(n==1,"death already recorded or inventory conflict");tx.execute("INSERT INTO deaths(character,snapshot,killed_by) VALUES(?1,?2,?3)",params![ch.id,serde_json::to_string(&ch)?,killed_by])?;tx.commit()?;Ok(())}).await
    }
    pub async fn fame(&self, account: i32, id: i32) -> Result<String> {
        self.call(move|db|{let (snapshot,killer):(String,String)=db.query_row("SELECT snapshot,killed_by FROM deaths JOIN characters ON characters.id=deaths.character WHERE characters.account=?1 AND deaths.character=?2",params![account,id],|r|Ok((r.get(0)?,r.get(1)?)))?;let c:Character=serde_json::from_str(&snapshot)?;ensure!(c.created_at>0,"creation time was not recorded by the early development binary for this character");Ok(format!("<Fame><Char id=\"{}\"><Account><Name>{}</Name></Account><ObjectType>782</ObjectType><Level>{}</Level><Tex1>0</Tex1><Tex2>0</Tex2></Char><CreatedOn>{}</CreatedOn><KilledBy>{}</KilledBy><TotalFame>{}</TotalFame><Shots>{}</Shots><ShotsThatDamage>{}</ShotsThatDamage><TilesUncovered>{}</TilesUncovered><MonsterKills>{}</MonsterKills><GodKills>0</GodKills><OryxKills>0</OryxKills><QuestsCompleted>{}</QuestsCompleted><PirateCavesCompleted>0</PirateCavesCompleted><UndeadLairsCompleted>0</UndeadLairsCompleted><AbyssOfDemonsCompleted>0</AbyssOfDemonsCompleted><SnakePitsCompleted>0</SnakePitsCompleted><SpiderDensCompleted>0</SpiderDensCompleted><SpriteWorldsCompleted>0</SpriteWorldsCompleted><TombsCompleted>0</TombsCompleted><LevelUpAssists>0</LevelUpAssists><BaseFame>{}</BaseFame></Fame>",c.id,xml_escape(&c.name),c.level,c.created_at,xml_escape(&killer),c.fame,c.shots,c.hits,c.tiles,c.kills,c.quests,c.fame))}).await
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
    pub fn xml(&self, w: &Object) -> String {
        let mut chars = String::new();
        for c in &self.characters {
            chars += &format!(
                "<Char id=\"{}\"><ObjectType>782</ObjectType><Level>{}</Level><Exp>{}</Exp><CurrentFame>{}</CurrentFame><Tex1>0</Tex1><Tex2>0</Tex2><Equipment>{}</Equipment><MaxHitPoints>{}</MaxHitPoints><HitPoints>{}</HitPoints><MaxMagicPoints>{}</MaxMagicPoints><MagicPoints>{}</MagicPoints><Attack>{}</Attack><Defense>{}</Defense><Speed>{}</Speed><Dexterity>{}</Dexterity><HpRegen>{}</HpRegen><MpRegen>{}</MpRegen></Char>",
                c.id,
                c.level,
                c.exp,
                c.fame,
                c.items
                    .iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
                w.hp,
                c.hp,
                w.mp,
                c.mp,
                w.attack,
                w.defense,
                w.speed,
                w.dexterity,
                w.vitality,
                w.wisdom
            );
        }
        format!(
            "<Chars nextCharId=\"{}\" maxNumChars=\"1\">{chars}<Account><AccountId>{}</AccountId><Name>{}</Name><NameChosen/><Stats><Fame>0</Fame><TotalFame>0</TotalFame><ClassStats objectType=\"782\"><BestLevel>{}</BestLevel><BestFame>0</BestFame></ClassStats></Stats><Credits>0</Credits><NextCharSlotPrice>600</NextCharSlotPrice></Account><Servers><Server><Name>Local development</Name><DNS>127.0.0.1</DNS><Lat>0</Lat><Long>0</Long><Usage>0</Usage></Server></Servers><News/></Chars>",
            self.next_id,
            self.id,
            xml_escape(&self.name),
            self.best_level
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn inventory_is_versioned_and_death_is_permanent() {
        let s = Store::open(Path::new(":memory:")).unwrap();
        let c = crate::content::Content::load(Path::new("data")).unwrap();
        let a = s.list("test-account".into()).await.unwrap();
        let mut ch = s
            .create(a.id, c.get(crate::content::WIZARD).unwrap())
            .await
            .unwrap();
        let mut inv = ch.items.clone();
        inv[4] = -1;
        ch.revision = s.inventory(ch.id, a.id, 0, inv.clone()).await.unwrap();
        assert!(s.inventory(ch.id, a.id, 0, inv.clone()).await.is_err());
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
