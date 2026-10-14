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
    #[serde(default)]
    pub test_state: bool,
    pub max_players: usize,
    pub behind_proxy: bool,
    #[serde(default)]
    pub shore_bots: bool,
    pub realm_seed: u64,
}
impl Config {
    pub fn load(args: &[String]) -> Result<(Self, bool)> {
        let mut file = None;
        let mut port = None;
        let mut control = false;
        let mut shore_bots = false;
        let mut client_dir = None;
        let mut state_dir = None;
        let mut test_state = false;
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--config" => {
                    ensure!(file.is_none(), "duplicate --config");
                    file = Some(args.next().context("--config requires a JSON file")?);
                }
                "--port" => {
                    ensure!(port.is_none(), "duplicate --port");
                    let value = args.next().context("--port requires a port number")?;
                    let value: u16 = value.parse().context("port must be between 1 and 65535")?;
                    ensure!(value != 0, "port must be between 1 and 65535");
                    port = Some(value);
                }
                "--dev-control" => {
                    ensure!(!control, "duplicate --dev-control");
                    control = true;
                }
                "--shore-bots" => {
                    ensure!(!shore_bots, "duplicate --shore-bots");
                    shore_bots = true;
                }
                "--client" => {
                    ensure!(client_dir.is_none(), "duplicate --client");
                    client_dir = Some(PathBuf::from(
                        args.next().context("--client requires a directory")?,
                    ));
                }
                "--state" | "--test-state" => {
                    ensure!(state_dir.is_none(), "duplicate state directory");
                    if arg == "--test-state" {
                        test_state = true;
                    }
                    state_dir = Some(PathBuf::from(
                        args.next().context("--state requires a directory")?,
                    ));
                }
                _ => anyhow::bail!(
                    "usage: openrotmg [--config FILE] [--port PORT] [--dev-control] [--shore-bots] [--client DIR] [--state DIR]"
                ),
            }
        }
        let mut config = if let Some(file) = file {
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
                test_state: false,
                max_players: 85,
                behind_proxy: false,
                shore_bots: false,
                realm_seed: rand::random(),
            }
        };
        if let Some(client) = client_dir {
            config.client = client;
        }
        if let Some(state) = state_dir {
            config.state = state;
        }
        if let Some(port) = port {
            config.bind.set_port(port);
            if file.is_none() {
                config.origin = format!("http://{}", config.bind);
            }
        }
        config.shore_bots |= shore_bots;
        config.test_state |= test_state;
        control |= test_state;
        ensure!(
            !config.test_state || control,
            "test state requires local development control"
        );
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
            !self.shore_bots || self.max_players >= 7,
            "adventuring bots require capacity for six bots plus a human"
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
        serde_json::from_str(r#"{"bind":"127.0.0.1:18923","origin":"https://openrotmg.org","data":"/opt/openrotmg/data","client":"/opt/openrotmg/client","state":"/var/lib/openrotmg","max_players":16,"behind_proxy":true,"realm_seed":12351}"#).unwrap()
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
    fn optional_local_port() {
        let (default, _) = Config::load(&[]).unwrap();
        assert_eq!(default.bind.to_string(), "127.0.0.1:18923");
        assert_eq!(default.origin, "http://127.0.0.1:18923");
        for port in ["1", "8080", "65535"] {
            let (custom, _) = Config::load(&["--port".into(), port.into()]).unwrap();
            assert_eq!(custom.bind.to_string(), format!("127.0.0.1:{port}"));
            assert_eq!(custom.origin, format!("http://127.0.0.1:{port}"));
        }
        assert!(Config::load(&["--port".into()]).is_err());
        for port in ["0", "-1", "65536", "abc", "8080.5"] {
            assert!(Config::load(&["--port".into(), port.into()]).is_err());
        }
        assert!(
            Config::load(&[
                "--port".into(),
                "8080".into(),
                "--port".into(),
                "8081".into()
            ])
            .is_err()
        );
    }
    #[test]
    fn shore_bots_are_opt_in_and_reserve_human_capacity() {
        assert!(!Config::load(&[]).unwrap().0.shore_bots);
        assert!(Config::load(&["--shore-bots".into()]).unwrap().0.shore_bots);
        assert!(Config::load(&["--shore-bots".into(), "--shore-bots".into()]).is_err());
        let mut production = config();
        production.shore_bots = true;
        production.validate(false).unwrap();
        assert!(production.validate(true).is_err());
        production.max_players = 6;
        assert!(production.validate(false).is_err());
        production.max_players = 7;
        production.validate(false).unwrap();
    }
    #[test]
    fn explicit_state_does_not_change_client_or_origin() {
        let (original, _) = Config::load(&[]).unwrap();
        let (isolated, _) = Config::load(&["--state".into(), "/tmp/qa-state".into()]).unwrap();
        assert_eq!(
            (isolated.state, isolated.client, isolated.origin),
            (
                PathBuf::from("/tmp/qa-state"),
                original.client,
                original.origin
            )
        );
    }
    #[test]
    fn malformed_state_arguments_are_rejected() {
        for args in [
            vec!["--state"],
            vec!["--state", "relative"],
            vec!["--state", "/tmp/a", "--state", "/tmp/b"],
        ] {
            assert!(Config::load(&args.into_iter().map(String::from).collect::<Vec<_>>()).is_err());
        }
    }
    #[test]
    fn local_launcher_remains_explicit() {
        Config::load(&[]).unwrap();
        assert!(Config::load(&["--hosted-client".into()]).is_err());
        assert!(Config::load(&["--client".into()]).is_err());
        assert!(Config::load(&["--client".into(), "relative".into()]).is_err());
        assert!(
            Config::load(&[
                "--client".into(),
                "/tmp/client".into(),
                "--hosted-client".into()
            ])
            .is_err()
        );
        let (local, _) = Config::load(&["--client".into(), "/tmp/client".into()]).unwrap();
        assert_eq!(local.client, PathBuf::from("/tmp/client"));
        Config::load(&["--dev-control".into()]).unwrap();
        assert!(Config::load(&["--dev-control".into(), "--dev-control".into()]).is_err());
    }
}
