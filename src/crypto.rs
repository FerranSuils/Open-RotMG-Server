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
fn patch_abc(abc: &[u8], replacement: &[String; 4]) -> Result<(Vec<u8>, usize)> {
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
    let count = u30(abc, &mut p)?;
    let mut out = abc[..p].to_vec();
    let mut changed = 0;
    for _ in 1..count {
        let n = u30(abc, &mut p)? as usize;
        let s = abc.get(p..p + n).context("truncated ABC string")?;
        p += n;
        if let Some(i) = OLD.iter().position(|v| v.as_bytes() == s) {
            put_u30(&mut out, replacement[i].len() as u32);
            out.extend(replacement[i].as_bytes());
            changed += 1;
        } else {
            put_u30(&mut out, n as u32);
            out.extend(s);
        }
    }
    out.extend(&abc[p..]);
    Ok((out, changed))
}
fn patch_swf(source: &[u8], replacement: &[String; 4]) -> Result<Vec<u8>> {
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
    while p < body.len() {
        let h = u16::from_le_bytes(
            body.get(p..p + 2)
                .context("truncated SWF tag")?
                .try_into()?,
        );
        p += 2;
        let tag = h >> 6;
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
            let (abc, c) = patch_abc(&data[offset..], replacement)?;
            changed += c;
            let mut d = data[..offset].to_vec();
            d.extend(abc);
            d
        } else {
            data.to_vec()
        };
        if patched.len() < 63 {
            out.extend(((tag << 6) | patched.len() as u16).to_le_bytes())
        } else {
            out.extend(((tag << 6) | 63).to_le_bytes());
            out.extend((patched.len() as u32).to_le_bytes())
        }
        out.extend(patched);
    }
    ensure!(
        changed == 4,
        "expected four public-key string fragments, found {changed}"
    );
    let mut result = source[..8].to_vec();
    result[4..8].copy_from_slice(&((out.len() + 8) as u32).to_le_bytes());
    if &source[..3] == b"CWS" {
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(&out)?;
        result.extend(z.finish()?);
    } else {
        result.extend(out);
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
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
