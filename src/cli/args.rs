use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,

}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Install the service (admin required)
    Install,
    /// Uninstall the service (admin required)
    Uninstall,
    /// Start the service
    Start,
    /// Stop the service
    Stop
}