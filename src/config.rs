use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Debug, Deserialize)]
struct Log {
    path: String,
}

#[derive(Debug, Deserialize)]
struct Service {
    active: bool,
}

#[derive(Debug, Deserialize)]
struct Config {
    log: Log,
    service: Service,
}

pub fn load_config<P: AsRef<Path>>() -> Result<Config, anyhow::Error> {
    let text = fs::read_to_string("Config.toml")?;
    let cfg: Config = toml::from_str(&text)?;
    Ok(cfg)
}
