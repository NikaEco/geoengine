use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;

use crate::docker::client::DockerClient;

#[derive(Subcommand)]
pub enum ImageCommands {
    /// Import a Docker image from a tar file (for air-gapped environments)
    Import {
        /// Path to the tar file containing the Docker image
        tarfile: PathBuf,

        /// Tag to apply to the imported image
        #[arg(short, long)]
        tag: Option<String>,
    },

    /// List all Docker images
    List {
        /// Filter by image name
        #[arg(short, long)]
        filter: Option<String>,

        /// Show all images including intermediate layers
        #[arg(short, long)]
        all: bool,
    },

    /// Pull an image from a registry
    Pull {
        /// Image name and tag (e.g., ubuntu:latest)
        image: String,
    },

    /// Remove a Docker image
    Remove {
        /// Image name, ID, or tag to remove
        image: String,

        /// Force removal even if containers are using the image
        #[arg(short, long)]
        force: bool,
    },

    /// Export a Docker image to a tar file (for transfer to air-gapped systems)
    Export {
        /// Image name or ID to export
        image: String,

        /// Output file path
        #[arg(short, long)]
        output: PathBuf,
    },
}

impl ImageCommands {
    pub async fn execute(self) -> Result<()> {
        let client = DockerClient::new().await?;

        match self {
            Self::Import { tarfile, tag } => {
                import_image(&client, &tarfile, tag.as_deref()).await
            }
            Self::List { filter, all } => list_images(&client, filter.as_deref(), all).await,
            Self::Pull { image } => pull_image(&client, &image).await,
            Self::Remove { image, force } => remove_image(&client, &image, force).await,
            Self::Export { image, output } => export_image(&client, &image, &output).await,
        }
    }
}

async fn import_image(client: &DockerClient, tarfile: &PathBuf, tag: Option<&str>) -> Result<()> {
    println!(
        "{} Importing image from {}...",
        "=>".blue().bold(),
        tarfile.display()
    );

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message("Loading image...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let image_id = client
        .import_image(tarfile, tag)
        .await
        .context("Failed to import image")?;

    pb.finish_and_clear();
    println!(
        "{} Successfully imported image: {}",
        "✓".green().bold(),
        image_id.cyan()
    );

    Ok(())
}

async fn list_images(client: &DockerClient, filter: Option<&str>, all: bool) -> Result<()> {
    let images = client
        .list_images(filter, all)
        .await
        .context("Failed to list images")?;

    if images.is_empty() {
        println!("{}", "No images found".yellow());
        return Ok(());
    }

    println!(
        "{:<50} {:<20} {:<15} {}",
        "REPOSITORY:TAG".bold(),
        "IMAGE ID".bold(),
        "SIZE".bold(),
        "CREATED".bold()
    );
    println!("{}", "-".repeat(100));

    for image in images {
        let repo_tag = image
            .repo_tags
            .first()
            .map(|s| s.as_str())
            .unwrap_or("<none>");
        let id = &image.id[7..19]; // Short ID
        let size = format_size(image.size);
        let created = format_timestamp(image.created);

        println!("{:<50} {:<20} {:<15} {}", repo_tag, id, size, created);
    }

    Ok(())
}

async fn pull_image(client: &DockerClient, image: &str) -> Result<()> {
    println!("{} Pulling image {}...", "=>".blue().bold(), image.cyan());

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message("Downloading...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    client
        .pull_image(image)
        .await
        .context("Failed to pull image")?;

    pb.finish_and_clear();
    println!(
        "{} Successfully pulled image: {}",
        "✓".green().bold(),
        image.cyan()
    );

    Ok(())
}

async fn remove_image(client: &DockerClient, image: &str, force: bool) -> Result<()> {
    println!("{} Removing image {}...", "=>".blue().bold(), image.cyan());

    client
        .remove_image(image, force)
        .await
        .context("Failed to remove image")?;

    println!(
        "{} Successfully removed image: {}",
        "✓".green().bold(),
        image.cyan()
    );

    Ok(())
}

async fn export_image(client: &DockerClient, image: &str, output: &PathBuf) -> Result<()> {
    println!(
        "{} Exporting image {} to {}...",
        "=>".blue().bold(),
        image.cyan(),
        output.display()
    );

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message("Exporting...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    client
        .export_image(image, output)
        .await
        .context("Failed to export image")?;

    pb.finish_and_clear();
    println!(
        "{} Successfully exported image to: {}",
        "✓".green().bold(),
        output.display()
    );

    Ok(())
}

fn format_size(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn format_timestamp(timestamp: i64) -> String {
    use chrono::{DateTime, Utc};
    let dt = DateTime::<Utc>::from_timestamp(timestamp, 0);
    dt.map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "Unknown".to_string())
}
