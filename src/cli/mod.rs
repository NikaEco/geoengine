pub mod deploy;
pub mod image;
pub mod project;
pub mod run;
pub mod service;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "geoengine")]
#[command(author = "GeoEngine Team")]
#[command(version)]
#[command(about = "Docker-based isolated runtime manager for geospatial workloads", long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage Docker images (import, export, list, pull, remove)
    Image {
        #[command(subcommand)]
        command: image::ImageCommands,
    },

    /// Run a container with GPU support and volume mounts
    Run(run::RunArgs),

    /// Manage GeoEngine projects
    Project {
        #[command(subcommand)]
        command: project::ProjectCommands,
    },

    /// Deploy images to GCP Artifact Registry
    Deploy {
        #[command(subcommand)]
        command: deploy::DeployCommands,
    },

    /// Manage the proxy service for ArcGIS/QGIS integration
    Service {
        #[command(subcommand)]
        command: service::ServiceCommands,
    },
}

impl Cli {
    pub async fn execute(self) -> Result<()> {
        match self.command {
            Commands::Image { command } => command.execute().await,
            Commands::Run(args) => args.execute().await,
            Commands::Project { command } => command.execute().await,
            Commands::Deploy { command } => command.execute().await,
            Commands::Service { command } => command.execute().await,
        }
    }
}
