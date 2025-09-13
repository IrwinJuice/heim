use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Debug, Deserialize)]
pub struct Log {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct AsService {
    pub active: bool,
}

#[derive(Debug, Deserialize)]
pub struct Host {
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub log: Log,
    pub service: AsService,
    pub host: Host,
}

pub fn load_config() -> Result<Config, anyhow::Error> {
    let text = fs::read_to_string("Config.toml")?;
    let cfg: Config = toml::from_str(&text)?;
    Ok(cfg)
}
