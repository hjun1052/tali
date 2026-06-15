use crate::doctor;
use crate::logs;
use crate::runner::{self, RunnerOptions};
use crate::self_test;
use crate::store::Store;
use anyhow::{bail, Result};
use clap::{Command, CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use std::collections::HashMap;
use std::env;
use std::io;

#[derive(Debug, Parser)]
#[command(name = "tali", version, about = "AI-friendly command manifest runner")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Copy a manifest into the global manifest store.
    Add {
        /// Path to a manifest TOML file.
        path: String,
    },
    /// List global manifests.
    List,
    /// Run a manifest by short ID or name.
    Run {
        /// Short ID or manifest name.
        id_or_name: String,
        /// Show what would happen without executing.
        #[arg(long)]
        dry_run: bool,
        /// Skip approval prompt.
        #[arg(long)]
        yes: bool,
        /// Provide an input non-interactively as key=value. Repeatable.
        #[arg(long = "input", value_name = "KEY=VALUE")]
        inputs: Vec<String>,
        /// Provide an input from an environment variable as key=ENV_VAR. Repeatable.
        #[arg(long = "input-env", value_name = "KEY=ENV_VAR")]
        input_envs: Vec<String>,
    },
    /// Inspect a manifest without executing it.
    Inspect {
        /// Short ID or manifest name.
        id_or_name: String,
        /// Print machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show latest or specific run logs.
    Logs {
        /// Use "latest", a run ID, or "follow".
        run_id: String,
        /// Target run for "tali logs follow <latest|run-id>".
        follow_target: Option<String>,
        /// Print the raw run JSON.
        #[arg(long, conflicts_with = "for_ai")]
        json: bool,
        /// Print a compact JSON summary intended for AI repair.
        #[arg(long, conflicts_with = "json")]
        for_ai: bool,
    },
    /// Capture environment and common tool information.
    Doctor {
        /// Print machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Verify this Tali installation can use its data directory and core helpers.
    SelfTest {
        /// Print machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Generate shell completion scripts.
    Completions {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: Shell,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse_from(rewrite_shortcut_args(env::args().collect()));
    let cwd = env::current_dir()?;

    match cli.command {
        Commands::Add { path } => {
            let store = Store::new()?;
            let entry = store.add_manifest(path.as_ref())?;
            println!("Added manifest:");
            println!("ID: {}", entry.id);
            println!("Name: {}", entry.name);
            println!("Run:");
            println!("tali {}", entry.id);
        }
        Commands::List => {
            let store = Store::new()?;
            let manifests = store.list_manifests()?;
            if manifests.is_empty() {
                println!("No global manifests.");
            } else {
                println!("ID  Name  Created  Description");
                for manifest in manifests {
                    println!(
                        "{}  {}  {}  {}",
                        manifest.id,
                        manifest.name,
                        manifest.created_at,
                        manifest.description.unwrap_or_default()
                    );
                }
            }
        }
        Commands::Run {
            id_or_name,
            dry_run,
            yes,
            inputs,
            input_envs,
        } => {
            let store = Store::new()?;
            let source = store.resolve_manifest(&id_or_name, &cwd)?;
            let result = runner::run_manifest(
                &store,
                &source,
                RunnerOptions {
                    yes,
                    dry_run,
                    provided_inputs: parse_inputs(&inputs, &input_envs)?,
                },
            )?;
            if matches!(
                result.status,
                crate::logs::RunStatus::Failed | crate::logs::RunStatus::Aborted
            ) && !dry_run
            {
                std::process::exit(1);
            }
        }
        Commands::Inspect { id_or_name, json } => {
            let store = Store::new()?;
            let source = store.resolve_manifest(&id_or_name, &cwd)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&source.manifest)?);
            } else {
                runner::inspect_manifest(&source.manifest);
            }
        }
        Commands::Logs {
            run_id,
            follow_target,
            json,
            for_ai,
        } => {
            let store = Store::new()?;
            if run_id == "follow" {
                if json || for_ai {
                    bail!("logs follow cannot be combined with --json or --for-ai");
                }
                let Some(target) = follow_target else {
                    bail!("logs follow requires 'latest' or a run ID");
                };
                let run_id = resolve_log_run_id(&store, &target, true)?;
                let run_dir = store.run_dir(&run_id);
                logs::follow_events(&run_dir)?;
            } else {
                if follow_target.is_some() {
                    bail!("unexpected extra logs argument");
                }
                let run_id = resolve_log_run_id(&store, &run_id, false)?;
                let run_dir = store.run_dir(&run_id);
                let run = logs::read_run_log(&run_dir)?;
                if json {
                    logs::print_run_json(&run)?;
                } else if for_ai {
                    logs::print_ai_summary(&run, &run_dir)?;
                } else {
                    logs::print_run_summary(&run, &run_dir);
                }
            }
        }
        Commands::Doctor { json } => {
            let info = doctor::capture();
            if json {
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                doctor::print_doctor(&info);
            }
        }
        Commands::SelfTest { json } => {
            let store = Store::new()?;
            let report = self_test::run(&store);
            if json {
                self_test::print_json(&report)?;
            } else {
                self_test::print_report(&report);
            }
            if report.status == self_test::SelfTestStatus::Failed {
                std::process::exit(1);
            }
        }
        Commands::Completions { shell } => {
            let mut command = command();
            generate(shell, &mut command, "tali", &mut io::stdout());
        }
    }

    Ok(())
}

pub fn command() -> Command {
    Cli::command()
}

fn parse_inputs(inputs: &[String], input_envs: &[String]) -> Result<HashMap<String, String>> {
    let mut parsed = HashMap::new();
    for input in inputs {
        let Some((key, value)) = input.split_once('=') else {
            bail!("input values must use KEY=VALUE syntax");
        };
        if key.trim().is_empty() {
            bail!("input key cannot be empty");
        }
        insert_input(&mut parsed, key, value.to_string())?;
    }
    for input_env in input_envs {
        let Some((key, env_name)) = input_env.split_once('=') else {
            bail!("input environment values must use KEY=ENV_VAR syntax");
        };
        if key.trim().is_empty() {
            bail!("input key cannot be empty");
        }
        if env_name.trim().is_empty() {
            bail!("environment variable name cannot be empty");
        }
        let value = env::var(env_name)
            .map_err(|_| anyhow::anyhow!("environment variable '{env_name}' is not set"))?;
        insert_input(&mut parsed, key, value)?;
    }
    Ok(parsed)
}

fn insert_input(parsed: &mut HashMap<String, String>, key: &str, value: String) -> Result<()> {
    if parsed.insert(key.to_string(), value).is_some() {
        bail!("input '{key}' was provided more than once");
    }
    Ok(())
}

fn resolve_log_run_id(store: &Store, selector: &str, prefer_running: bool) -> Result<String> {
    if selector != "latest" {
        return Ok(selector.to_string());
    }
    if prefer_running {
        if let Some(run_id) = store.latest_running_run_id()? {
            return Ok(run_id);
        }
    }
    store.latest_run_id()
}

fn rewrite_shortcut_args(args: Vec<String>) -> Vec<String> {
    let known = [
        "add",
        "list",
        "run",
        "inspect",
        "logs",
        "doctor",
        "self-test",
        "completions",
        "help",
        "--help",
        "-h",
        "--version",
        "-V",
    ];
    if args.len() > 1 && !args[1].starts_with('-') && !known.contains(&args[1].as_str()) {
        let mut rewritten = Vec::with_capacity(args.len() + 1);
        rewritten.push(args[0].clone());
        rewritten.push("run".to_string());
        rewritten.extend(args.into_iter().skip(1));
        rewritten
    } else {
        args
    }
}
