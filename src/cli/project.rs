use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;

use crate::config::project::ProjectConfig;
use crate::config::settings::Settings;
use crate::docker::client::DockerClient;

#[derive(Subcommand)]
pub enum ProjectCommands {
    /// Initialize a new geoengine.yaml configuration file
    Init {
        /// Project name
        #[arg(short, long)]
        name: Option<String>,

        /// Output directory (defaults to current directory)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Register a project directory with GeoEngine
    Register {
        /// Path to the project directory containing geoengine.yaml
        path: PathBuf,

        /// Override project name
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Unregister a project
    Unregister {
        /// Project name to unregister
        name: String,
    },

    /// List all registered projects
    List,

    /// Build the Docker image for a project
    Build {
        /// Project name (or path to project directory)
        project: String,

        /// Don't use cache when building
        #[arg(long)]
        no_cache: bool,

        /// Build arguments (format: KEY=VALUE)
        #[arg(long, value_name = "KEY=VALUE")]
        build_arg: Vec<String>,
    },

    /// Run a script defined in the project
    Run {
        /// Project name
        project: String,

        /// Script name (defaults to 'default')
        #[arg(default_value = "default")]
        script: String,

        /// Additional arguments to pass to the script
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Show project configuration details
    Show {
        /// Project name
        project: String,
    },
}

impl ProjectCommands {
    pub async fn execute(self) -> Result<()> {
        match self {
            Self::Init { name, output } => init_project(name.as_deref(), output.as_ref()).await,
            Self::Register { path, name } => register_project(&path, name.as_deref()).await,
            Self::Unregister { name } => unregister_project(&name).await,
            Self::List => list_projects().await,
            Self::Build {
                project,
                no_cache,
                build_arg,
            } => build_project(&project, no_cache, &build_arg).await,
            Self::Run {
                project,
                script,
                args,
            } => run_project(&project, &script, &args).await,
            Self::Show { project } => show_project(&project).await,
        }
    }
}

async fn init_project(name: Option<&str>, output: Option<&PathBuf>) -> Result<()> {
    let output_dir = output
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let config_path = output_dir.join("geoengine.yaml");

    if config_path.exists() {
        anyhow::bail!("geoengine.yaml already exists in {}", output_dir.display());
    }

    let project_name = name
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            output_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("my-project")
                .to_string()
        });

    let template = ProjectConfig::template(&project_name);
    let yaml = serde_yaml::to_string(&template)?;

    std::fs::write(&config_path, yaml)?;

    println!(
        "{} Created {} in {}",
        "✓".green().bold(),
        "geoengine.yaml".cyan(),
        output_dir.display()
    );
    println!("\nNext steps:");
    println!("  1. Edit geoengine.yaml to configure your project");
    println!("  2. Run {} to register the project", "geoengine project register .".cyan());
    println!("  3. Run {} to build the Docker image", "geoengine project build <name>".cyan());

    Ok(())
}

async fn register_project(path: &PathBuf, name: Option<&str>) -> Result<()> {
    let path = path.canonicalize()
        .with_context(|| format!("Directory not found: {}", path.display()))?;

    let config_path = path.join("geoengine.yaml");
    if !config_path.exists() {
        anyhow::bail!(
            "No geoengine.yaml found in {}. Run 'geoengine project init' first.",
            path.display()
        );
    }

    let config = ProjectConfig::load(&config_path)?;
    let project_name = name.map(|s| s.to_string()).unwrap_or(config.name.clone());

    let mut settings = Settings::load()?;
    settings.register_project(&project_name, &path)?;
    settings.save()?;

    println!(
        "{} Registered project '{}' at {}",
        "✓".green().bold(),
        project_name.cyan(),
        path.display()
    );

    Ok(())
}

async fn unregister_project(name: &str) -> Result<()> {
    let mut settings = Settings::load()?;
    settings.unregister_project(name)?;
    settings.save()?;

    println!(
        "{} Unregistered project '{}'",
        "✓".green().bold(),
        name.cyan()
    );

    Ok(())
}

async fn list_projects() -> Result<()> {
    let settings = Settings::load()?;
    let projects = settings.list_projects();

    if projects.is_empty() {
        println!("{}", "No projects registered".yellow());
        println!(
            "\nRegister a project with: {}",
            "geoengine project register <path>".cyan()
        );
        return Ok(());
    }

    println!("{:<25} {}", "NAME".bold(), "PATH".bold());
    println!("{}", "-".repeat(80));

    for (name, path) in projects {
        let status = if path.join("geoengine.yaml").exists() {
            "✓".green()
        } else {
            "✗".red()
        };
        println!("{} {:<23} {}", status, name, path.display());
    }

    Ok(())
}

async fn build_project(project: &str, no_cache: bool, build_args: &[String]) -> Result<()> {
    let settings = Settings::load()?;
    let project_path = settings.get_project_path(project)?;
    let config = ProjectConfig::load(&project_path.join("geoengine.yaml"))?;

    let client = DockerClient::new().await?;

    println!(
        "{} Building project '{}'...",
        "=>".blue().bold(),
        project.cyan()
    );

    let dockerfile = project_path.join(
        config
            .build
            .as_ref()
            .and_then(|b| b.dockerfile.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("Dockerfile"),
    );

    if !dockerfile.exists() {
        anyhow::bail!("Dockerfile not found: {}", dockerfile.display());
    }

    let context = project_path.join(
        config
            .build
            .as_ref()
            .and_then(|b| b.context.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("."),
    );

    let image_tag = format!("geoengine-{}:latest", config.name);

    // Parse build args
    let mut args: std::collections::HashMap<String, String> = config
        .build
        .as_ref()
        .and_then(|b| b.args.clone())
        .unwrap_or_default();

    for arg in build_args {
        let parts: Vec<&str> = arg.splitn(2, '=').collect();
        if parts.len() == 2 {
            args.insert(parts[0].to_string(), parts[1].to_string());
        }
    }

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message("Building image...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    client
        .build_image(&dockerfile, &context, &image_tag, &args, no_cache)
        .await?;

    pb.finish_and_clear();
    println!(
        "{} Successfully built image: {}",
        "✓".green().bold(),
        image_tag.cyan()
    );

    Ok(())
}

async fn run_project(project: &str, script: &str, args: &[String]) -> Result<()> {
    let settings = Settings::load()?;
    let project_path = settings.get_project_path(project)?;
    let config = ProjectConfig::load(&project_path.join("geoengine.yaml"))?;

    let script_cmd = config
        .scripts
        .as_ref()
        .and_then(|s| s.get(script))
        .ok_or_else(|| anyhow::anyhow!("Script '{}' not found in project", script))?;

    let image_tag = format!("geoengine-{}:latest", config.name);

    // Build run args from project config
    let mut run_args = crate::cli::run::RunArgs {
        image: image_tag,
        command: vec!["/bin/sh".to_string(), "-c".to_string()],
        mount: Vec::new(),
        gpu: config.runtime.as_ref().map(|r| r.gpu).unwrap_or(false),
        env: Vec::new(),
        env_file: None,
        memory: config.runtime.as_ref().and_then(|r| r.memory.clone()),
        cpus: config.runtime.as_ref().and_then(|r| r.cpus),
        shm_size: config.runtime.as_ref().and_then(|r| r.shm_size.clone()),
        workdir: config.runtime.as_ref().and_then(|r| r.workdir.clone()),
        detach: false,
        name: None,
        rm: true,
        tty: true,
    };

    // Build command with script and args
    let full_command = if args.is_empty() {
        script_cmd.clone()
    } else {
        format!("{} {}", script_cmd, args.join(" "))
    };
    run_args.command.push(full_command);

    // Add mounts from config
    if let Some(runtime) = &config.runtime {
        if let Some(mounts) = &runtime.mounts {
            for mount in mounts {
                let host_path = if mount.host.starts_with("./") {
                    project_path.join(&mount.host[2..])
                } else {
                    PathBuf::from(&mount.host)
                };

                let mount_str = if mount.readonly.unwrap_or(false) {
                    format!("{}:{}:ro", host_path.display(), mount.container)
                } else {
                    format!("{}:{}", host_path.display(), mount.container)
                };
                run_args.mount.push(mount_str);
            }
        }

        // Add environment variables
        if let Some(env) = &runtime.environment {
            for (key, value) in env {
                run_args.env.push(format!("{}={}", key, value));
            }
        }
    }

    println!(
        "{} Running script '{}' for project '{}'...",
        "=>".blue().bold(),
        script.cyan(),
        project.cyan()
    );

    run_args.execute().await
}

async fn show_project(project: &str) -> Result<()> {
    let settings = Settings::load()?;
    let project_path = settings.get_project_path(project)?;
    let config = ProjectConfig::load(&project_path.join("geoengine.yaml"))?;

    println!("{}: {}", "Name".bold(), config.name);
    println!("{}: {}", "Version".bold(), config.version.as_deref().unwrap_or("N/A"));
    println!("{}: {}", "Path".bold(), project_path.display());

    if let Some(base) = &config.base_image {
        println!("{}: {}", "Base Image".bold(), base);
    }

    if let Some(runtime) = &config.runtime {
        println!("\n{}:", "Runtime Configuration".bold().underline());
        println!("  GPU: {}", if runtime.gpu { "enabled" } else { "disabled" });
        if let Some(mem) = &runtime.memory {
            println!("  Memory: {}", mem);
        }
        if let Some(cpus) = runtime.cpus {
            println!("  CPUs: {}", cpus);
        }
        if let Some(workdir) = &runtime.workdir {
            println!("  Workdir: {}", workdir);
        }
    }

    if let Some(scripts) = &config.scripts {
        println!("\n{}:", "Scripts".bold().underline());
        for (name, cmd) in scripts {
            println!("  {}: {}", name.cyan(), cmd);
        }
    }

    if let Some(gis) = &config.gis {
        if let Some(tools) = &gis.tools {
            println!("\n{}:", "GIS Tools".bold().underline());
            for tool in tools {
                println!("  {} - {}", tool.name.cyan(), tool.label.as_deref().unwrap_or(""));
            }
        }
    }

    Ok(())
}
