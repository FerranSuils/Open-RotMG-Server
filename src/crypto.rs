use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use rand::rngs::OsRng;
use rsa::{
    Pkcs1v15Encrypt, RsaPrivateKey,
    pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding},
};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
};

mod gold;

const REFERENCE: &str = "07ae4fc3ca021a5bf08bbf94f057f7620be18b85e01abd763b3a198382e8fa48";
const OLD: [&str; 4] = [
    "MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQCqccYHTj4QATbK1m4UVgcTeEYt",
    "DZwZxwdayVTFs1jAwKWzoDt/CXhYUX2cEJnA93T4h6a4ysTfUrgBFZ3Nsb4W3/4C",
    "rc2TxjOWQOoWQnhzblZEiTcAmIdwdq8AfuZWjRpGhOzHDPK9hcgySrzqD9HJAUsb",
    "G2ZzU5zrxHtEPC0nUQIDAQAB\n",
];

pub struct Keys {
    private: RsaPrivateKey,
}
impl Keys {
    pub fn load(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("server.pem");
        if !path.exists() {
            let key = RsaPrivateKey::new(&mut OsRng, 2048)?;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options
                .open(&path)?
                .write_all(key.to_pkcs8_pem(LineEnding::LF)?.as_bytes())?;
            tracing::info!(path=%path.display(),"created local server keypair; never commit this directory");
        }
        let private = RsaPrivateKey::from_pkcs8_pem(&std::fs::read_to_string(&path)?)
            .context("invalid server key; refusing to replace it")?;
        Ok(Self { private })
    }
    pub fn decrypt(&self, s: &str) -> Result<String> {
        // The Flash encryptor emits zero blocks for an empty password or secret.
        if s.is_empty() {
            return Ok(String::new());
        }
        let bytes = STANDARD.decode(s)?;
        let clear = self
            .private
            .decrypt(Pkcs1v15Encrypt, &bytes)
            .context("HELLO credential does not match this server's public key")?;
        Ok(String::from_utf8(clear)?)
    }
    pub fn client(&self, path: &Path) -> Result<Vec<u8>> {
        let source = std::fs::read(path)?;
        let hash = format!("{:x}", Sha256::digest(&source));
        ensure!(hash == REFERENCE, "wrong reference client: {hash}");
        let public = self.private.to_public_key().to_public_key_der()?;
        let b64 = STANDARD.encode(public.as_bytes());
        ensure!(b64.len() > 192, "unexpected public key encoding");
        let replacement = [
            b64[..64].to_owned(),
            b64[64..128].to_owned(),
            b64[128..192].to_owned(),
            format!("{}\n", &b64[192..]),
        ];
        patch_swf(&source, &replacement)
    }
}
fn u30(data: &[u8], pos: &mut usize) -> Result<u32> {
    let mut v = 0;
    for shift in (0..35).step_by(7) {
        let b = *data.get(*pos).context("truncated ABC integer")?;
        *pos += 1;
        v |= ((b & 127) as u32) << shift;
        if b & 128 == 0 {
            return Ok(v);
        }
    }
    bail!("ABC integer overflow")
}
fn put_u30(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let mut b = (v & 127) as u8;
        v >>= 7;
        if v != 0 {
            b |= 128
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}
fn patch_abc(abc: &[u8], replacement: &[String; 4]) -> Result<(Vec<u8>, usize, usize, usize)> {
    ensure!(abc.len() >= 4, "truncated ABC header");
    let mut p = 4;
    for _ in 0..2 {
        let n = u30(abc, &mut p)?;
        for _ in 1..n {
            u30(abc, &mut p)?;
        }
    }
    let doubles = u30(abc, &mut p)?;
    p += doubles.saturating_sub(1) as usize * 8;
    ensure!(p <= abc.len(), "truncated double pool");
    let pool = p;
    let count = u30(abc, &mut p)?;
    let gold = abc.windows(gold::METHOD.len()).any(|v| v == gold::METHOD);
    let mut out = abc[..pool].to_vec();
    put_u30(&mut out, count + if gold { 2 } else { 0 });
    let mut changed = 0;
    let mut music = 0;
    for _ in 1..count {
        let n = u30(abc, &mut p)? as usize;
        let s = abc.get(p..p + n).context("truncated ABC string")?;
        p += n;
        if let Some(i) = OLD.iter().position(|v| v.as_bytes() == s) {
            put_u30(&mut out, replacement[i].len() as u32);
            out.extend(replacement[i].as_bytes());
            changed += 1;
        } else if s == b"/music/sorc.mp3" {
            // Both launchers already redirect Appspot /sfx/ to same-origin audio.
            let path = b"/sfx/music/sorc.mp3";
            put_u30(&mut out, path.len() as u32);
            out.extend(path);
            music += 1;
        } else if gold && s == b"Buy Realm Gold" {
            let title = b"Monetary's Realm Gold";
            put_u30(&mut out, title.len() as u32);
            out.extend(title);
        } else {
            put_u30(&mut out, n as u32);
            out.extend(s);
        }
    }
    if gold {
        for text in [b"/gold/".as_slice(), b"Get Gold".as_slice()] {
            put_u30(&mut out, text.len() as u32);
            out.extend(text);
        }
    }
    out.extend(&abc[p..]);
    if gold {
        gold::patch(&mut out, count, count + 1)?;
    }
    Ok((out, changed, music, usize::from(gold)))
}
const LOADING_METHOD: &[u8] = b"\x05\x03\x09\x0a\x76\x27\x26\xd5\x10\x06\x00\x00\x1c\x77\x1d\x87\x26\xa6\xd6\xd0\x30\xd1\x2a\x11\x03\x00\x00\x29\xd0\x76\x12\x3b\x00\x00\xd0\x66\xaa\x30\x10\x06\x00\x00\x85\x96\xb0\xad\xc0\xc7\xd2\x11\x37\x00\x00\x76\xd2\x2a\x12\x0d\x00\x00\x10\x06\x00\x00\x30\x73\x87\x82\xd7\x1d\x29\xd2\x76\x11\x1f\x00\x00\x2a\x12\x1a\x00\x00\xd2\x11\x15\x00\x00\x29\xd1\x12\x18\x00\x00\xd0\x66\xaf\x0f\x66\xaf\x4e\x60\xfc\x29\x60\x8b\x1f\xa3\xb0\x12\x0a\x00\x00\xd1\x12\x05\x00\x00\xd0\x4f\xc6\x3c\x00\x47\x00\x00";
fn patch_loading(data: &mut [u8]) -> Result<usize> {
    let positions: Vec<_> = data
        .windows(LOADING_METHOD.len())
        .enumerate()
        .filter_map(|(i, bytes)| (bytes == LOADING_METHOD).then_some(i))
        .collect();
    ensure!(positions.len() <= 1, "ambiguous loading-readiness method");
    if let Some(&offset) = positions.first() {
        // Socket initialization can precede the embedded loading movie's COMPLETE event.
        let guard = &mut data[offset + 5 + 43..offset + 5 + 88];
        guard.fill(0x02);
        guard[..16]
            .copy_from_slice(b"\x12\x46\x00\x00\xd0\x66\xaf\x0f\x12\x3e\x00\x00\x10\x1d\x00\x00");
    }
    Ok(positions.len())
}
/// Custom textures and object/ground XML (pixel-art/inject.py): replacement tag bodies keyed by
/// (tag code, character id) for the pinned client.
const OVERLAY: &[u8] = include_bytes!("../data/client-overlay.bin");
fn overlay() -> Result<std::collections::BTreeMap<(u16, u16), &'static [u8]>> {
    let mut tags = std::collections::BTreeMap::new();
    let mut p = 0;
    while p < OVERLAY.len() {
        let head = OVERLAY.get(p..p + 6).context("truncated client overlay")?;
        let n = u32::from_le_bytes(head[2..6].try_into()?) as usize;
        let body = OVERLAY
            .get(p + 6..p + 6 + n)
            .context("truncated client overlay tag")?;
        ensure!(body.len() >= 2, "client overlay tag lacks a character id");
        let key = (
            u16::from_le_bytes([head[0], head[1]]),
            u16::from_le_bytes([body[0], body[1]]),
        );
        ensure!(
            tags.insert(key, body).is_none(),
            "duplicate client overlay tag {key:?}"
        );
        p += 6 + n;
    }
    Ok(tags)
}
/// DefineBits, DefineBitsJPEG2, DefineBitsJPEG3, DefineBitsLossless, DefineBitsJPEG4.
const BITMAPS: [u16; 5] = [6, 21, 35, 20, 90];
/// DefineBitsLossless2: the tag kind custom bitmaps are written as.
const LOSSLESS_ALPHA: u16 = 36;
/// Client mods (client-mods/build.py): sha256 of the pinned client's main ABC, then the modified ABC that replaces
/// it; the key, music and gold patches below then apply to the modified one.
const CLIENT_ABC: &[u8] = include_bytes!("../data/client-abc.bin");
/// Body of an extra DoABC tag (the compiled mod helper) placed right after the main ABC; may be empty.
const CLIENT_HELPER: &[u8] = include_bytes!("../data/client-helper.bin");
/// Development only: OPENROTMG_CLIENT_MODS=<dir> reads client-abc.bin and client-helper.bin from that directory
/// at start-up instead of the copies built in, so client-mods changes need a restart rather than a rebuild.
fn client_mod(name: &str, built_in: &'static [u8]) -> &'static [u8] {
    match std::env::var_os("OPENROTMG_CLIENT_MODS") {
        Some(dir) => std::fs::read(std::path::Path::new(&dir).join(name))
            .map(|bytes| &*Box::leak(bytes.into_boxed_slice()))
            .unwrap_or(built_in),
        None => built_in,
    }
}
fn client_abc() -> &'static [u8] {
    static ABC: std::sync::OnceLock<&'static [u8]> = std::sync::OnceLock::new();
    ABC.get_or_init(|| client_mod("client-abc.bin", CLIENT_ABC))
}
fn client_helper() -> &'static [u8] {
    static HELPER: std::sync::OnceLock<&'static [u8]> = std::sync::OnceLock::new();
    HELPER.get_or_init(|| client_mod("client-helper.bin", CLIENT_HELPER))
}
/// The modified main ABC when `abc` is the original it was built from.
fn modded_abc(abc: &[u8]) -> Option<&'static [u8]> {
    let (hash, modded) = client_abc().split_at_checked(32)?;
    (Sha256::digest(abc).as_slice() == hash).then_some(modded)
}
fn patch_swf(source: &[u8], replacement: &[String; 4]) -> Result<Vec<u8>> {
    let mut overlay = overlay()?;
    let overlaid = overlay.len();
    ensure!(source.len() >= 9, "truncated reference SWF");
    let mut body = Vec::new();
    match &source[..3] {
        b"FWS" => body.extend(&source[8..]),
        b"CWS" => {
            ZlibDecoder::new(&source[8..]).read_to_end(&mut body)?;
        }
        _ => bail!("unsupported SWF compression"),
    }
    let rect_bits = (body[0] >> 3) as usize;
    let start = (5 + 4 * rect_bits).div_ceil(8) + 4;
    ensure!(start <= body.len(), "truncated SWF movie header");
    let mut out = body[..start].to_vec();
    let mut p = start;
    let mut changed = 0;
    let mut loading = 0;
    let mut music = 0;
    let mut gold = 0;
    let mut modded = 0;
    while p < body.len() {
        let h = u16::from_le_bytes(
            body.get(p..p + 2)
                .context("truncated SWF tag")?
                .try_into()?,
        );
        p += 2;
        let mut tag = h >> 6;
        let n = if h & 63 == 63 {
            let n = u32::from_le_bytes(
                body.get(p..p + 4)
                    .context("truncated tag length")?
                    .try_into()?,
            ) as usize;
            p += 4;
            n
        } else {
            (h & 63) as usize
        };
        let data = body.get(p..p + n).context("truncated tag body")?;
        p += n;
        let patched = if tag == 82 {
            ensure!(data.len() > 4, "truncated DoABC");
            let offset = 5 + data[4..]
                .iter()
                .position(|b| *b == 0)
                .context("unterminated DoABC name")?;
            let original = &data[offset..];
            let source_abc = match modded_abc(original) {
                Some(abc) => {
                    modded += 1;
                    abc
                }
                None => original,
            };
            let (mut abc, c, m, g) = patch_abc(source_abc, replacement)?;
            changed += c;
            music += m;
            gold += g;
            loading += patch_loading(&mut abc)?;
            let mut d = data[..offset].to_vec();
            d.extend(abc);
            d
        } else if let Some(body) = data
            .get(..2)
            .and_then(|id| overlay.remove(&(tag, u16::from_le_bytes([id[0], id[1]]))))
        {
            body.to_vec()
        } else if let Some(body) = data
            .get(..2)
            .filter(|_| BITMAPS.contains(&tag))
            .and_then(|id| overlay.remove(&(LOSSLESS_ALPHA, u16::from_le_bytes([id[0], id[1]]))))
        {
            // A lossless bitmap with alpha may replace any bitmap kind (e.g. a JPEG) of the same id.
            tag = LOSSLESS_ALPHA;
            body.to_vec()
        } else {
            data.to_vec()
        };
        if patched.len() < 63 {
            out.extend(((tag << 6) | patched.len() as u16).to_le_bytes())
        } else {
            out.extend(((tag << 6) | 63).to_le_bytes());
            out.extend((patched.len() as u32).to_le_bytes())
        }
        let main_abc = tag == 82 && modded == 1 && patched.len() > 600_000;
        out.extend(patched);
        let helper = client_helper();
        if main_abc && !helper.is_empty() {
            out.extend(((82u16 << 6) | 63).to_le_bytes());
            out.extend((helper.len() as u32).to_le_bytes());
            out.extend(helper);
        }
    }
    ensure!(
        modded == 1,
        "expected the pinned client's main ABC for the client mods, matched {modded}"
    );
    ensure!(
        changed == 4,
        "expected four public-key string fragments, found {changed}"
    );
    ensure!(music == 1, "expected one music URL, found {music}");
    ensure!(
        gold == 1,
        "expected one gold navigation patch, found {gold}"
    );
    ensure!(
        loading == 1,
        "expected one exact loading-readiness method, found {loading}"
    );
    ensure!(
        overlay.is_empty(),
        "client overlay tags absent from the pinned client: {:?}",
        overlay.keys().collect::<Vec<_>>()
    );
    tracing::info!(tags = overlaid, "applied custom content overlay to client");
    // Always served compressed: the uncompressed movie is twice the download.
    let mut result = source[..8].to_vec();
    result[..3].copy_from_slice(b"CWS");
    result[4..8].copy_from_slice(&((out.len() + 8) as u32).to_le_bytes());
    let mut z = ZlibEncoder::new(result, Compression::best());
    z.write_all(&out)?;
    Ok(z.finish()?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loading_guard_requires_exact_unique_method_and_preserves_layout() {
        let mut method = LOADING_METHOD.to_vec();
        let before = method.clone();
        assert_eq!(patch_loading(&mut method).unwrap(), 1);
        assert_eq!(method.len(), before.len());
        assert_eq!(&method[..48], &before[..48]);
        assert_eq!(&method[93..], &before[93..]);
        assert_eq!(
            &method[48..64],
            b"\x12\x46\x00\x00\xd0\x66\xaf\x0f\x12\x3e\x00\x00\x10\x1d\x00\x00"
        );
        assert!(method[64..93].iter().all(|byte| *byte == 0x02));
        assert_eq!(patch_loading(&mut method).unwrap(), 0);
        let mut duplicate = before.repeat(2);
        assert!(patch_loading(&mut duplicate).is_err());
        assert_eq!(duplicate, before.repeat(2));
        for i in 0..before.len() {
            let mut corrupt = before.clone();
            corrupt[i] ^= 1;
            assert_eq!(patch_loading(&mut corrupt).unwrap(), 0);
        }
    }
    #[test]
    fn audio_route_patch_preserves_other_abc_strings_and_tail() {
        fn abc(strings: &[&str]) -> Vec<u8> {
            let mut bytes = vec![16, 0, 46, 0, 0, 0, 0];
            put_u30(&mut bytes, strings.len() as u32 + 1);
            for string in strings {
                put_u30(&mut bytes, string.len() as u32);
                bytes.extend(string.as_bytes());
            }
            bytes.extend([0, 1, 2, 3]);
            bytes
        }
        let replacement = OLD.map(String::from);
        let original = abc(&[OLD[0], "/music/sorc.mp3", "untouched", OLD[3]]);
        let (patched, keys, music, gold) = patch_abc(&original, &replacement).unwrap();
        assert_eq!((keys, music, gold), (2, 1, 0));
        assert_eq!(
            patched,
            abc(&[OLD[0], "/sfx/music/sorc.mp3", "untouched", OLD[3]])
        );
        let (again, _, music, _) = patch_abc(&patched, &replacement).unwrap();
        assert_eq!(music, 0);
        assert_eq!(again, patched);
    }
    #[test]
    fn public_key_patch_preserves_reference() {
        let dir = std::env::temp_dir().join(format!("openrotmg-key-test-{}", std::process::id()));
        let k = Keys::load(&dir).unwrap();
        let input = Path::new("../client/client.swf");
        if input.exists() {
            let before = std::fs::read(input).unwrap();
            let patched = k.client(input).unwrap();
            assert_ne!(patched, before);
            assert_eq!(std::fs::read(input).unwrap(), before);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
