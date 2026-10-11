use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{net::SocketAddr, path::PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub bind: SocketAddr,
    pub origin: String,
    pub data: PathBuf,
    pub client: PathBuf,
    pub state: PathBuf,
    pub max_players: usize,
    pub behind_proxy: bool,
}
impl Config {
    pub fn load(args: &[String]) -> Result<(Self, bool)> {
        let mut file = None;
        let mut control = false;
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--config" => {
                    ensure!(file.is_none(), "duplicate --config");
                    file = Some(args.next().context("--config requires a JSON file")?);
                }
                "--dev-control" => {
                    ensure!(!control, "duplicate --dev-control");
                    control = true;
                }
                _ => anyhow::bail!("usage: openrotmg [--config FILE] [--dev-control]"),
            }
        }
        let config = if let Some(file) = file {
            serde_json::from_slice(
                &std::fs::read(file).with_context(|| format!("read config {file}"))?,
            )?
        } else {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            Self {
                bind: "127.0.0.1:18923".parse()?,
                origin: "http://127.0.0.1:18923".into(),
                data: root.join("data"),
                client: root
                    .parent()
                    .context("workspace parent missing")?
                    .join("client"),
                state: root.join(".local"),
                max_players: 85,
                behind_proxy: false,
            }
        };
        config.validate(control)?;
        Ok((config, control))
    }
    fn validate(&self, control: bool) -> Result<()> {
        ensure!(
            self.bind.ip().is_loopback(),
            "backend bind must be loopback; terminate public TLS at the proxy"
        );
        let uri: axum::http::Uri = self.origin.parse().context("invalid public origin")?;
        ensure!(
            uri.authority().is_some()
                && uri.path() == "/"
                && uri.query().is_none()
                && !self.origin.ends_with('/'),
            "origin must be scheme://host[:port], without a path"
        );
        let local = uri.host().is_some_and(|h| {
            h == "localhost"
                || h.parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        ensure!(
            uri.scheme_str() == Some("https") || (local && uri.scheme_str() == Some("http")),
            "public origin requires HTTPS"
        );
        ensure!(
            !control || (local && !self.behind_proxy),
            "development control cannot be enabled behind a public proxy"
        );
        ensure!(
            (1..=85).contains(&self.max_players),
            "max_players must be between 1 and 85"
        );
        ensure!(
            [&self.data, &self.client, &self.state]
                .iter()
                .all(|p| p.is_absolute()),
            "data, client and state paths must be absolute"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        serde_json::from_str(r#"{"bind":"127.0.0.1:18923","origin":"https://openrotmg.org","data":"/opt/openrotmg/data","client":"/opt/openrotmg/client","state":"/var/lib/openrotmg","max_players":16,"behind_proxy":true}"#).unwrap()
    }
    #[test]
    fn explicit_production_config() {
        config().validate(false).unwrap();
        assert!(config().validate(true).is_err());
        assert!(Config::load(&["--config".into()]).is_err());
        assert!(Config::load(&["--unknown".into()]).is_err());
        let mut c = config();
        c.bind = "0.0.0.0:18923".parse().unwrap();
        assert!(c.validate(false).is_err());
        c = config();
        c.origin = "http://openrotmg.org".into();
        assert!(c.validate(false).is_err());
        c = config();
        c.state = "relative".into();
        assert!(c.validate(false).is_err());
    }
    #[test]
    fn local_launcher_remains_explicit() {
        Config::load(&[]).unwrap();
        Config::load(&["--dev-control".into()]).unwrap();
        assert!(Config::load(&["--dev-control".into(), "--dev-control".into()]).is_err());
    }
}
