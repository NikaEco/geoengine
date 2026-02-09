use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::project::ProjectConfig;
use crate::config::settings::Settings;
use crate::docker::client::DockerClient;
use crate::docker::gpu::GpuConfig;
use crate::cli::run::ContainerConfig;

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
    List {
        /// Output as JSON (for programmatic use)
        #[arg(long)]
        json: bool,
    },

    /// List GIS tools defined in a project (JSON output)
    Tools {
        /// Project name
        project: String,
    },

    /// Run a GIS tool from a project
    RunTool {
        /// Project name
        project: String,

        /// Tool name (as defined in geoengine.yaml gis.tools)
        tool: String,

        /// Input parameters (format: KEY=VALUE, repeatable)
        /// Keys are mapped to script flags using the tool's input definitions.
        /// If the input has a `map_to` field, that is used as the flag name;
        /// otherwise the input's `name` is used.
        #[arg(short, long = "input", value_name = "KEY=VALUE")]
        inputs: Vec<String>,

        /// Output directory for results (mounted to /output in container)
        #[arg(short, long)]
        output_dir: Option<String>,

        /// Emit structured JSON result to stdout on completion
        #[arg(long)]
        json: bool,
    },

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
    /// Dispatches the selected `ProjectCommands` variant to its corresponding handler.
    ///
    /// Executes the action represented by this enum instance (init, register, unregister, list,
    /// tools, run-tool, build, run, or show) and returns the result of that operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures::executor::block_on;
    ///
    /// // Construct a command (adjust fields as needed for other variants)
    /// let cmd = ProjectCommands::List { json: true };
    /// let result = block_on(cmd.execute());
    /// assert!(result.is_ok());
    /// ```
    pub async fn execute(self) -> Result<()> {
        match self {
            Self::Init { name, output } => init_project(name.as_deref(), output.as_ref()).await,
            Self::Register { path, name } => register_project(&path, name.as_deref()).await,
            Self::Unregister { name } => unregister_project(&name).await,
            Self::List { json } => list_projects(json).await,
            Self::Tools { project } => list_tools(&project).await,
            Self::RunTool {
                project,
                tool,
                inputs,
                output_dir,
                json,
            } => run_tool(&project, &tool, &inputs, output_dir.as_deref(), json).await,
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

/// Unregisters a project by its registration name from the saved settings.

///

/// This removes the project entry stored in the user's settings and persists the change.

/// On success a confirmation message is printed to stdout.

///

/// # Parameters

///

/// - `name`: The registration name of the project to remove (the key used when registering).

///

/// # Returns

///

/// `Ok(())` on success, `Err` if loading, unregistering, or saving the settings fails.

///

/// # Examples

///

/// ```

/// # use anyhow::Result;

/// # async fn doc() -> Result<()> {

/// unregister_project("my-project").await?;

/// # Ok(())

/// # }

/// ```
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

/// Lists registered projects either as a human-readable table or as JSON.
///
/// When `json` is `true`, prints a JSON array of objects with `name`, `path`, and
/// `tools_count` for each registered project. When `json` is `false`, prints a
/// formatted table showing each project's name, path, and a status marker
/// indicating whether a `geoengine.yaml` exists at the project path.
///
/// # Examples
///
/// ```
/// // Run the async function from a synchronous context for demonstration.
/// let rt = tokio::runtime::Runtime::new().unwrap();
/// rt.block_on(async {
///     // Print projects as JSON
///     let _ = crate::cli::project::list_projects(true).await;
/// });
/// ```
async fn list_projects(json: bool) -> Result<()> {
    let settings = Settings::load()?;
    let projects = settings.list_projects();

    if json {
        let mut entries: Vec<ProjectListEntry> = Vec::new();
        for (name, path) in &projects {
            let config_path = path.join("geoengine.yaml");
            let tools_count = if config_path.exists() {
                ProjectConfig::load(&config_path)
                    .ok()
                    .and_then(|c| c.gis)
                    .and_then(|g| g.tools)
                    .map(|t| t.len())
                    .unwrap_or(0)
            } else {
                0
            };
            entries.push(ProjectListEntry {
                name: name.to_string(),
                path: path.display().to_string(),
                tools_count,
            });
        }
        println!("{}", serde_json::to_string(&entries)?);
        return Ok(());
    }

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

/// Builds a Docker image for the given project using the project's build configuration.
///
/// The function loads the project's configuration (geoengine.yaml), determines the Dockerfile
/// and build context (using config overrides when present), merges build arguments from the
/// config with `build_args` (CLI `KEY=VALUE` strings override config values), runs the Docker
/// build, and tags the resulting image as `geoengine-{name}:latest`. Progress is shown during the
/// build and a success message with the image tag is printed on completion.
///
/// # Parameters
/// - `project`: The registered project name to build (as stored in settings).
/// - `no_cache`: When `true`, instructs Docker to build without using the cache.
/// - `build_args`: Extra build arguments in the form `KEY=VALUE`; values here override config args.
///
/// # Returns
/// Returns `Ok(())` on successful build, or an error if loading settings/config, locating files,
/// or the Docker build itself fails.
///
/// # Examples
///
/// ```ignore
/// # async fn doc_example() -> anyhow::Result<()> {
/// // Build with an extra build argument and without using the cache
/// build_project("my-project", true, &["VERSION=1.2.3".to_string()]).await?;
/// # Ok(())
/// # }
/// ```
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

// ---------------------------------------------------------------------------
// Shared run options for run_project and run_tool
// ---------------------------------------------------------------------------

/// Options for running a project script
#[derive(Default)]
struct RunOptions {
    /// Extra mounts to add (host_path, container_path, readonly)
    extra_mounts: Vec<(String, String, bool)>,
    /// Extra environment variables
    extra_env: HashMap<String, String>,
    /// Output as JSON (logs to stderr, JSON result to stdout)
    json_output: bool,
    /// Output directory (for collecting output files in JSON mode)
    output_dir: Option<String>,
    /// Display name for the operation (e.g., "tool 'classify'" vs "script 'train'")
    display_name: String,
}

/// Runs the named script for a registered project with default run options.
///
/// This delegates to `run_project_with_options` using default mounts, environment,
/// and JSON/output settings while setting the displayed name to the script.
///
/// # Examples
///
/// ```no_run
/// # use anyhow::Result;
/// # async fn example() -> Result<()> {
/// run_project("my-project", "build", &Vec::new()).await?;
/// # Ok(())
/// # }
/// ```
///
/// # Returns
///
/// `Ok(())` if the script completed successfully, or an error describing the failure.
async fn run_project(project: &str, script: &str, args: &[String]) -> Result<()> {
    let options = RunOptions {
        display_name: format!("script '{}'", script),
        ..Default::default()
    };
    run_project_with_options(project, script, args, options).await
}

/// Runs a project's script inside a container using the provided execution options.
///
/// Loads the project configuration, prepares environment variables and mounts (from both
/// the project runtime config and the supplied `options`), constructs the container command
/// (including safely escaped arguments), detects optional GPU settings, runs the container,
/// and reports the result either as human-readable output or as a JSON object (when
/// `options.json_output` is true). If the container exits with a non-zero code this function
/// will terminate the process with that exit code.
///
/// # Parameters
///
/// - `project` — The registered project name to resolve the project path and configuration.
/// - `script` — The key of the script to execute as defined in the project's `scripts`.
/// - `args` — Additional command-line arguments that will be shell-escaped and appended to the script.
/// - `options` — Runtime options controlling mounts, environment variables, JSON output mode, output directory,
///   and the display name used in status messages.
///
/// # Returns
///
/// `Ok(())` on successful execution (or after printing JSON result). The process will exit with the
/// container's exit code if the container terminates with a non-zero status.
///
/// # Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// # async fn example() -> anyhow::Result<()> {
/// let options = crate::cli::project::RunOptions {
///     extra_mounts: vec![(String::from("/host/path"), String::from("/container/path"), true)],
///     extra_env: std::collections::HashMap::new(),
///     json_output: false,
///     output_dir: Some(String::from("/tmp/output")),
///     display_name: String::from("my-script"),
/// };
/// // Runs the "build" script of the "example-project" with two arguments.
/// crate::cli::project::run_project_with_options("example-project", "build", &vec![String::from("arg1"), String::from("arg2")], options).await?;
/// # Ok(()) }
/// ```
async fn run_project_with_options(
    project: &str,
    script: &str,
    args: &[String],
    options: RunOptions,
) -> Result<()> {
    let settings = Settings::load()?;
    let project_path = settings.get_project_path(project)?;
    let config = ProjectConfig::load(&project_path.join("geoengine.yaml"))?;

    // Get the script command
    let script_cmd = config
        .scripts
        .as_ref()
        .and_then(|s| s.get(script))
        .ok_or_else(|| anyhow::anyhow!("Script '{}' not found in project", script))?;

    // Build environment variables from config + extra
    let mut env_vars: HashMap<String, String> = config
        .runtime
        .as_ref()
        .and_then(|r| r.environment.clone())
        .unwrap_or_default();
    env_vars.extend(options.extra_env);

    // Build mounts from config
    let mut mounts: Vec<(String, String, bool)> = Vec::new();
    if let Some(runtime) = &config.runtime {
        if let Some(mount_configs) = &runtime.mounts {
            for m in mount_configs {
                let host_path = if m.host.starts_with("./") {
                    project_path.join(&m.host[2..])
                } else {
                    PathBuf::from(&m.host)
                };
                mounts.push((
                    host_path.to_string_lossy().to_string(),
                    m.container.clone(),
                    m.readonly.unwrap_or(false),
                ));
            }
        }
    }

    // Add extra mounts from options
    mounts.extend(options.extra_mounts);

    // Build full command with args
    let full_command = if args.is_empty() {
        script_cmd.clone()
    } else {
        let escaped_args: Vec<String> = args.iter().map(|a| shell_escape(a)).collect();
        format!("{} {}", script_cmd, escaped_args.join(" "))
    };

    // Configure GPU
    let gpu_config = if config.runtime.as_ref().map(|r| r.gpu).unwrap_or(false) {
        GpuConfig::detect().await.ok()
    } else {
        None
    };

    // Build ContainerConfig
    let image_tag = format!("geoengine-{}:latest", config.name);
    let container_config = ContainerConfig {
        image: image_tag,
        command: Some(vec!["/bin/sh".to_string(), "-c".to_string(), full_command]),
        env_vars,
        mounts,
        gpu_config,
        memory: config.runtime.as_ref().and_then(|r| r.memory.clone()),
        cpus: config.runtime.as_ref().and_then(|r| r.cpus),
        shm_size: config.runtime.as_ref().and_then(|r| r.shm_size.clone()),
        workdir: config.runtime.as_ref().and_then(|r| r.workdir.clone()),
        name: None,
        remove_on_exit: true,
        detach: false,
        tty: !options.json_output, // TTY off in JSON mode
    };

    // Print status message
    if !options.json_output {
        eprintln!(
            "{} Running {} for project '{}'...",
            "=>".blue().bold(),
            options.display_name.cyan(),
            project.cyan()
        );
    }

    // Run the container
    let client = DockerClient::new().await?;
    let exit_code = if options.json_output {
        client.run_container_attached_to_stderr(&container_config).await?
    } else {
        client.run_container_attached(&container_config).await?
    };

    // Handle output
    if options.json_output {
        let files = collect_output_files(options.output_dir.as_deref());
        let result = RunToolResult {
            status: if exit_code == 0 { "completed".to_string() } else { "failed".to_string() },
            exit_code,
            output_dir: options.output_dir.as_ref().map(|s| {
                Path::new(s)
                    .canonicalize()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| s.clone())
            }),
            files,
            error: if exit_code != 0 {
                Some(format!("Container exited with code {}", exit_code))
            } else {
                None
            },
        };
        println!("{}", serde_json::to_string(&result)?);
    } else if exit_code == 0 {
        eprintln!("{} Completed successfully", "✓".green().bold());
    } else {
        eprintln!("{} Failed with exit code {}", "✗".red().bold(), exit_code);
    }

    if exit_code != 0 {
        std::process::exit(exit_code as i32);
    }

    Ok(())
}

/// Prints detailed information about a registered project to standard output.
///
/// The output includes project name, version, path, optional base image,
/// runtime configuration (GPU, memory, CPUs, workdir), available scripts,
/// and configured GIS tools.
///
/// # Arguments
///
/// * `project` - The name of a registered project as stored in the user's settings.
///
/// # Returns
///
/// Returns `Ok(())` on success. Returns an error if loading settings fails,
/// the named project is not registered, or the project's `geoengine.yaml` cannot be read.
///
/// # Examples
///
/// ```
/// # tokio_test::block_on(async {
/// // Prints information for the project named "my-project"
/// let _ = geoengine_cli::cli::project::show_project("my-project").await;
/// # });
/// ```
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

// ---------------------------------------------------------------------------
// JSON output structs (used by --json flags and plugin integration)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ProjectListEntry {
    name: String,
    path: String,
    tools_count: usize,
}

#[derive(Serialize)]
struct ToolInfoJson {
    name: String,
    label: Option<String>,
    description: Option<String>,
    inputs: Option<Vec<ParameterInfoJson>>,
    outputs: Option<Vec<ParameterInfoJson>>,
}

#[derive(Serialize)]
struct ParameterInfoJson {
    name: String,
    label: Option<String>,
    map_to: Option<String>,
    param_type: String,
    required: bool,
    default: Option<serde_yaml::Value>,
    description: Option<String>,
    choices: Option<Vec<String>>,
}

#[derive(Serialize)]
struct RunToolResult {
    status: String,
    exit_code: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_dir: Option<String>,
    files: Vec<OutputFileInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct OutputFileInfo {
    name: String,
    path: String,
    size: u64,
}

// ---------------------------------------------------------------------------
// project tools <project>
// ---------------------------------------------------------------------------

/// Lists the GIS tools declared in a project's `geoengine.yaml` and prints them as JSON.
///
/// Loads the project's configuration, extracts each tool's metadata (name, label, description)
/// and its inputs/outputs (including parameter name, label, mapping, type, required flag,
/// default, description, and choices), then serializes the resulting array to stdout.
///
/// # Examples
///
/// ```
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     // Prints a JSON array of tools for the project named "my-project".
///     list_tools("my-project").await?;
///     Ok(())
/// }
/// ```
async fn list_tools(project: &str) -> Result<()> {
    let settings = Settings::load()?;
    let project_path = settings.get_project_path(project)?;
    let config = ProjectConfig::load(&project_path.join("geoengine.yaml"))?;

    let tools: Vec<ToolInfoJson> = config
        .gis
        .as_ref()
        .and_then(|g| g.tools.as_ref())
        .map(|tools| {
            tools
                .iter()
                .map(|t| ToolInfoJson {
                    name: t.name.clone(),
                    label: t.label.clone(),
                    description: t.description.clone(),
                    inputs: t.inputs.as_ref().map(|inputs| {
                        inputs
                            .iter()
                            .map(|i| ParameterInfoJson {
                                name: i.name.clone(),
                                label: i.label.clone(),
                                map_to: i.map_to.clone(),
                                param_type: i.param_type.clone(),
                                required: i.required.unwrap_or(true),
                                default: i.default.clone(),
                                description: i.description.clone(),
                                choices: i.choices.clone(),
                            })
                            .collect()
                    }),
                    outputs: t.outputs.as_ref().map(|outputs| {
                        outputs
                            .iter()
                            .map(|o| ParameterInfoJson {
                                name: o.name.clone(),
                                label: o.label.clone(),
                                map_to: o.map_to.clone(),
                                param_type: o.param_type.clone(),
                                required: o.required.unwrap_or(true),
                                default: o.default.clone(),
                                description: o.description.clone(),
                                choices: o.choices.clone(),
                            })
                            .collect()
                    }),
                })
                .collect()
        })
        .unwrap_or_default();

    println!("{}", serde_json::to_string(&tools)?);
    Ok(())
}

// ---------------------------------------------------------------------------
// project run-tool <project> <tool> --input KEY=VALUE ... [--output-dir PATH] [--json]
// ---------------------------------------------------------------------------

/// Executes a GIS tool defined in a project's configuration by mapping provided KEY=VALUE
/// inputs to the tool's script, mounting any file or directory inputs into the container,
/// optionally mounting an output directory at `/output` and setting `GEOENGINE_OUTPUT_DIR`,
/// and running the tool's script inside the project's runtime container.
///
/// The function:
/// - Loads the project configuration and locates the named tool.
/// - Parses `input_args` items of the form `KEY=VALUE`.
/// - For each input value that is an existing file or directory, mounts it read-only into the container
///   (`/inputs/<filename>` for files, `/mnt/input_N` for directories) and replaces the value with the
///   corresponding container path.
/// - Maps input names to command flags using each tool input's `map_to` (or the input name if absent)
///   and constructs script arguments as `--<flag> <value>`.
/// - If `output_dir` is provided, ensures it exists, mounts it at `/output`, and sets
///   `GEOENGINE_OUTPUT_DIR=/output` in the container environment.
/// - Applies extra mounts and environment variables and executes the tool script with the constructed arguments.
///
/// Returns `Ok(())` on successful execution, or an error with context on failure.
///
/// # Examples
///
/// ```
/// // Run the tool `convert` in project `myproj` with two inputs and capture output into ./out
/// # tokio::runtime::Runtime::new().unwrap().block_on(async {
/// let args = vec!["input1=file1.tif".to_string(), "threshold=0.5".to_string()];
/// run_tool("myproj", "convert", &args, Some("./out"), false).await?;
/// # Result::<(), anyhow::Error>::Ok(())
/// # }).unwrap();
/// ```
async fn run_tool(
    project: &str,
    tool_name: &str,
    input_args: &[String],
    output_dir: Option<&str>,
    json_output: bool,
) -> Result<()> {
    let settings = Settings::load()?;
    let project_path = settings.get_project_path(project)?;
    let config = ProjectConfig::load(&project_path.join("geoengine.yaml"))?;

    // 1. Find the tool definition to get the script name and input mappings
    let tool = config
        .gis
        .as_ref()
        .and_then(|g| g.tools.as_ref())
        .and_then(|tools| tools.iter().find(|t| t.name == tool_name))
        .ok_or_else(|| anyhow::anyhow!("Tool '{}' not found in project '{}'", tool_name, project))?
        .clone();

    // 2. Parse --input KEY=VALUE args into a HashMap
    let mut inputs: HashMap<String, String> = HashMap::new();
    for arg in input_args {
        let parts: Vec<&str> = arg.splitn(2, '=').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid input format: '{}'. Expected KEY=VALUE", arg);
        }
        inputs.insert(parts[0].to_string(), parts[1].to_string());
    }

    // 3. Build extra mounts and env vars
    let mut extra_mounts: Vec<(String, String, bool)> = Vec::new();
    let mut extra_env: HashMap<String, String> = HashMap::new();
    let mut input_counter = 0usize;

    // 4. Output directory mount
    if let Some(out_dir) = output_dir {
        std::fs::create_dir_all(out_dir)
            .with_context(|| format!("Failed to create output directory: {}", out_dir))?;
        let abs_out = Path::new(out_dir)
            .canonicalize()
            .with_context(|| format!("Failed to resolve output directory: {}", out_dir))?;
        extra_mounts.push((abs_out.to_string_lossy().to_string(), "/output".to_string(), false));
        extra_env.insert("GEOENGINE_OUTPUT_DIR".to_string(), "/output".to_string());
    }

    // 5. Build script arguments from inputs using tool's input definitions
    //    Each input becomes: --<flag_name> <value>
    //    where flag_name = input.map_to if set, otherwise input.name
    //    File/directory paths are auto-mounted and rewritten
    let mut script_args: Vec<String> = Vec::new();

    // Get tool input definitions for mapping
    let tool_inputs = tool.inputs.as_ref();

    for (key, value) in &inputs {
        // Find the input definition to get the flag name
        let flag_name = tool_inputs
            .and_then(|inputs| inputs.iter().find(|i| i.name == *key))
            .map(|i| i.map_to.as_ref().unwrap_or(&i.name).clone())
            .unwrap_or_else(|| key.clone());

        // Check if the value is a file or directory path that needs mounting
        let path = Path::new(value);
        let processed_value = if path.exists() {
            if path.is_file() {
                // Mount file read-only into /inputs/
                if let Some(filename) = path.file_name() {
                    let abs_path = path
                        .canonicalize()
                        .with_context(|| format!("Failed to resolve input path: {}", value))?;
                    let container_path = format!("/inputs/{}", filename.to_string_lossy());
                    extra_mounts.push((
                        abs_path.to_string_lossy().to_string(),
                        container_path.clone(),
                        true,
                    ));
                    container_path
                } else {
                    value.clone()
                }
            } else if path.is_dir() {
                // Mount directory read-only into /mnt/input_N/
                let abs_path = path
                    .canonicalize()
                    .with_context(|| format!("Failed to resolve input directory: {}", value))?;
                let container_path = format!("/mnt/input_{}", input_counter);
                input_counter += 1;
                extra_mounts.push((
                    abs_path.to_string_lossy().to_string(),
                    container_path.clone(),
                    true,
                ));
                container_path
            } else {
                value.clone()
            }
        } else {
            value.clone()
        };

        // Add --flag_name value to script args
        script_args.push(format!("--{}", flag_name));
        script_args.push(processed_value);
    }

    // 6. Build options and delegate to run_project_with_options
    let options = RunOptions {
        extra_mounts,
        extra_env,
        json_output,
        output_dir: output_dir.map(|s| s.to_string()),
        display_name: format!("tool '{}'", tool_name),
    };

    run_project_with_options(project, &tool.script, &script_args, options).await
}

/// Produces a shell-escaped string safe for inclusion in a POSIX shell command.
///
/// The returned string is single-quoted if it contains characters that would be
/// interpreted by the shell; embedded single quotes are escaped so the resulting
/// value is a valid single-quoted shell token.
///
/// # Parameters
///
/// - `s`: input string to escape.
///
/// # Returns
///
/// A `String` containing the escaped representation suitable for use in a shell command.
///
/// # Examples
///
/// ```
/// assert_eq!(shell_escape("simple"), "simple");
/// assert_eq!(shell_escape("has space"), "'has space'");
/// assert_eq!(shell_escape("a'b"), "'a'\\''b'");
/// ```
fn shell_escape(s: &str) -> String {
    // If the string contains special characters, wrap in single quotes
    // and escape any single quotes within
    if s.chars().any(|c| " \t\n\"'\\$`!*?[]{}();<>&|".contains(c)) {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}

/// Collects regular files in `output_dir` and returns their metadata.
///
/// If `output_dir` is `None` or cannot be read, an empty vector is returned.
/// Non-file entries and entries that fail to be read are ignored.
///
/// # Examples
///
/// ```
/// use std::fs::File;
/// use tempfile::tempdir;
///
/// let dir = tempdir().unwrap();
/// let file_path = dir.path().join("out.txt");
/// File::create(&file_path).unwrap();
///
/// let files = crate::cli::project::collect_output_files(Some(dir.path().to_string_lossy().as_ref()));
/// assert!(files.iter().any(|f| f.name == "out.txt"));
/// ```
fn collect_output_files(output_dir: Option<&str>) -> Vec<OutputFileInfo> {
    let Some(dir) = output_dir else {
        return Vec::new();
    };

    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut files = Vec::new();
    for entry in entries.flatten() {
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() {
                files.push(OutputFileInfo {
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: entry.path().to_string_lossy().to_string(),
                    size: metadata.len(),
                });
            }
        }
    }
    files
}