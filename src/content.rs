use anyhow::{Context, Result, bail, ensure};
use roxmltree::Node;
use serde::Serialize;
use std::{collections::BTreeMap, io::Read, path::Path};

pub const WIZARD: u16 = 0x030e;
#[derive(Clone, Debug, Serialize)]
pub struct Projectile {
    pub speed: f32,
    pub lifetime: f32,
    pub min: i32,
    pub max: i32,
    pub amplitude: f32,
    pub frequency: f32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Object {
    pub kind: u16,
    pub name: String,
    pub class: String,
    pub hp: i32,
    pub mp: i32,
    pub attack: i32,
    pub defense: i32,
    pub speed: i32,
    pub dexterity: i32,
    pub vitality: i32,
    pub wisdom: i32,
    pub equipment: Vec<i32>,
    pub slots: Vec<i32>,
    pub slot_type: i32,
    pub bonuses: Vec<(u8, i32)>,
    pub unsupported_equipment: bool,
    pub projectile: Option<Projectile>,
    pub shots: i32,
    pub arc: f32,
    pub rate: f32,
    pub mana_cost: i32,
    pub activate: String,
    pub heal: i32,
    pub enemy: bool,
    pub blocks: bool,
    pub destructible: bool,
}
#[derive(Clone)]
pub struct Content {
    pub objects: BTreeMap<u16, Object>,
    pub grounds: BTreeMap<u8, f32>,
}
fn child<'a, 'i>(n: Node<'a, 'i>, tag: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(tag))
}
fn text(n: Node<'_, '_>, tag: &str) -> String {
    child(n, tag)
        .and_then(|c| c.text())
        .unwrap_or("")
        .trim()
        .to_owned()
}
// Omitted optional fields have the client's XML defaults, not substitute content.
fn number(n: Node<'_, '_>, tag: &str, default: f32) -> Result<f32> {
    match child(n, tag).and_then(|c| c.text()) {
        None => Ok(default),
        Some(v) => {
            let x: f32 = v.trim().parse().with_context(|| {
                format!("{} {tag}: {v}", n.attribute("id").unwrap_or("projectile"))
            })?;
            ensure!(x.is_finite(), "non-finite XML number");
            Ok(x)
        }
    }
}
fn list(n: Node<'_, '_>, tag: &str) -> Result<Vec<i32>> {
    let s = text(n, tag);
    if s.is_empty() {
        return Ok(vec![]);
    }
    s.split(',')
        .map(|v| {
            let v = v.trim();
            if let Some(h) = v.strip_prefix("0x") {
                Ok(i32::from_str_radix(h, 16)?)
            } else {
                Ok(v.parse()?)
            }
        })
        .collect()
}
fn kind(s: &str) -> Result<u16> {
    if let Some(h) = s.strip_prefix("0x") {
        Ok(u16::from_str_radix(h, 16)?)
    } else {
        Ok(s.parse()?)
    }
}
impl Content {
    pub fn load(dir: &Path) -> Result<Self> {
        let data = std::fs::read_to_string(dir.join("objects.xml"))?;
        let doc = roxmltree::Document::parse(&data)?;
        let mut objects = BTreeMap::new();
        for n in doc.root_element().children().filter(|n| n.is_element()) {
            let id = kind(n.attribute("type").context("object has no type")?)?;
            let projectile = child(n, "Projectile")
                .map(|p| -> Result<Projectile> {
                    let fixed = number(p, "Damage", 0.)? as i32;
                    Ok(Projectile {
                        speed: number(p, "Speed", 100.)?,
                        lifetime: number(p, "LifetimeMS", 0.)?,
                        min: number(p, "MinDamage", fixed as f32)? as i32,
                        max: number(p, "MaxDamage", fixed as f32)? as i32,
                        amplitude: number(p, "Amplitude", 0.)?,
                        frequency: number(p, "Frequency", 1.)?,
                    })
                })
                .transpose()?;
            let o = Object {
                kind: id,
                name: n.attribute("id").context("object has no name")?.into(),
                class: text(n, "Class"),
                hp: number(n, "MaxHitPoints", 0.)? as i32,
                mp: number(n, "MaxMagicPoints", 0.)? as i32,
                attack: number(n, "Attack", 0.)? as i32,
                defense: number(n, "Defense", 0.)? as i32,
                speed: number(n, "Speed", 0.)? as i32,
                dexterity: number(n, "Dexterity", 0.)? as i32,
                vitality: number(n, "HpRegen", 0.)? as i32,
                wisdom: number(n, "MpRegen", 0.)? as i32,
                equipment: list(n, "Equipment")?,
                slots: list(n, "SlotTypes")?,
                slot_type: number(n, "SlotType", 0.)? as i32,
                bonuses: n
                    .children()
                    .filter(|x| {
                        x.has_tag_name("ActivateOnEquip") && x.text() == Some("IncrementStat")
                    })
                    .map(|x| -> Result<(u8, i32)> {
                        Ok((
                            x.attribute("stat")
                                .context("equipment stat missing")?
                                .parse()?,
                            x.attribute("amount")
                                .context("equipment amount missing")?
                                .parse()?,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?,
                unsupported_equipment: n.children().any(|x| {
                    x.has_tag_name("ActivateOnEquip") && x.text() != Some("IncrementStat")
                }),
                projectile,
                shots: number(n, "NumProjectiles", 1.)? as i32,
                arc: number(n, "ArcGap", 11.25)?,
                rate: number(n, "RateOfFire", 1.)?,
                mana_cost: number(n, "MpCost", 0.)? as i32,
                activate: text(n, "Activate"),
                heal: child(n, "Activate")
                    .and_then(|p| p.attribute("amount"))
                    .map(str::parse)
                    .transpose()?
                    .unwrap_or(0),
                enemy: child(n, "Enemy").is_some(),
                blocks: child(n, "OccupySquare").is_some() || child(n, "FullOccupy").is_some(),
                destructible: child(n, "Enemy").is_some(),
            };
            ensure!(
                objects.insert(id, o).is_none(),
                "duplicate object type {id:#x}"
            );
        }
        let data = std::fs::read_to_string(dir.join("grounds.xml"))?;
        let doc = roxmltree::Document::parse(&data)?;
        let mut grounds = BTreeMap::new();
        for n in doc.root_element().children().filter(|n| n.is_element()) {
            let id = u8::try_from(kind(n.attribute("type").context("ground type missing")?)?)?;
            let speed = if child(n, "NoWalk").is_some() {
                0.
            } else {
                number(n, "Speed", 1.)?
            };
            ensure!(
                grounds.insert(id, speed).is_none(),
                "duplicate ground type {id:#x}"
            );
        }
        ensure!(
            objects.len() == 2201 && grounds.len() == 173,
            "unexpected 123.5.1 content counts"
        );
        let out = Self { objects, grounds };
        let w = out.get(WIZARD)?;
        ensure!(
            w.hp > 0 && w.equipment.len() == 12 && w.slots.len() == 12,
            "invalid Wizard definition"
        );
        Ok(out)
    }
    pub fn get(&self, k: u16) -> Result<&Object> {
        self.objects
            .get(&k)
            .with_context(|| format!("unknown object type {k:#x}"))
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Cell {
    pub ground: u8,
    pub object: Option<u16>,
    pub config: String,
    pub region: u8,
}
#[derive(Clone, Debug, Serialize)]
pub struct Map {
    pub width: i32,
    pub height: i32,
    pub cells: Vec<Cell>,
    pub spawns: Vec<(f32, f32)>,
}
struct MapReader {
    data: Vec<u8>,
    pos: usize,
}
impl MapReader {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        ensure!(n <= self.data.len() - self.pos, "truncated map");
        let p = self.pos;
        self.pos += n;
        Ok(&self.data[p..p + n])
    }
    fn short(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into()?))
    }
    fn int(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn string(&mut self) -> Result<String> {
        let mut n = 0usize;
        for shift in (0..35).step_by(7) {
            let b = self.byte()?;
            n |= ((b & 127) as usize) << shift;
            if b & 128 == 0 {
                return Ok(std::str::from_utf8(self.take(n)?)?.to_owned());
            }
        }
        bail!("map string length overflow")
    }
}
impl Map {
    pub fn load(path: &Path, content: &Content) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let mut data = Vec::new();
        flate2::read::ZlibDecoder::new(bytes.as_slice())
            .take(16 * 1024 * 1024)
            .read_to_end(&mut data)?;
        let mut r = MapReader { data, pos: 0 };
        let count = r.short()?;
        ensure!(count > 0, "empty map dictionary");
        let mut dict = Vec::new();
        for _ in 0..count {
            let ground = u8::try_from(r.short()?)?;
            ensure!(
                content.grounds.contains_key(&ground),
                "unknown map ground {ground:#x}"
            );
            let name = r.string()?;
            let object = if name.is_empty() {
                None
            } else {
                Some(
                    content
                        .objects
                        .values()
                        .find(|o| o.name == name)
                        .with_context(|| format!("unknown map object {name}"))?
                        .kind,
                )
            };
            let config = r.string()?;
            let _terrain = r.byte()?;
            let region = r.byte()?;
            dict.push(Cell {
                ground,
                object,
                config,
                region,
            });
        }
        let width = r.int()?;
        let height = r.int()?;
        ensure!(
            width > 0 && height > 0 && width as i64 * height as i64 <= 1_000_000,
            "map dimensions exceed development limit"
        );
        let mut cells = Vec::new();
        let mut spawns = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let i = usize::try_from(r.short()?)?;
                let c = dict.get(i).context("invalid map dictionary index")?.clone();
                if c.region == 1 {
                    spawns.push((x as f32 + 0.5, y as f32 + 0.5));
                }
                cells.push(c);
            }
        }
        ensure!(r.pos == r.data.len(), "trailing map data");
        ensure!(!spawns.is_empty(), "map has no spawn cells");
        Ok(Self {
            width,
            height,
            cells,
            spawns,
        })
    }
    pub fn cell(&self, x: f32, y: f32) -> Option<&Cell> {
        if x < 0. || y < 0. || x >= self.width as f32 || y >= self.height as f32 {
            return None;
        }
        self.cells
            .get(y.floor() as usize * self.width as usize + x.floor() as usize)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn client_data_identity() {
        let c = Content::load(Path::new("data")).unwrap();
        let w = c.get(WIZARD).unwrap();
        assert_eq!(w.equipment[0], 0xa97);
        assert_eq!(w.equipment[1], 0xa2e);
        assert_eq!(w.hp, 100);
        assert!(c.grounds.contains_key(&0x61) && c.grounds.contains_key(&0x65));
    }
    #[test]
    fn candidate_map_is_lossless() {
        let c = Content::load(Path::new("data")).unwrap();
        let m = Map::load(Path::new("data/tutorial.wmap"), &c).unwrap();
        assert_eq!((m.width, m.height, m.cells.len()), (25, 201, 5025));
        assert_eq!(m.spawns.len(), 12);
        assert!(m.cells.iter().any(|c| c.object == Some(0x071e)));
    }
}
