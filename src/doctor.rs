use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolVersion {
    pub found: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformInfo {
    pub os: String,
    pub architecture: String,
    pub current_directory: String,
    pub shell: Option<String>,
    pub path: Option<String>,
    pub tali_version: String,
    pub rustc_version: Option<String>,
    pub tools: BTreeMap<String, ToolVersion>,
}

pub fn capture() -> PlatformInfo {
    let tools = common_tools()
        .into_iter()
        .map(|tool| (tool.to_string(), capture_tool(tool)))
        .collect();

    PlatformInfo {
        os: env::consts::OS.to_string(),
        architecture: env::consts::ARCH.to_string(),
        current_directory: env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "<unknown>".to_string()),
        shell: env::var("SHELL").ok().or_else(|| env::var("ComSpec").ok()),
        path: env::var("PATH").ok(),
        tali_version: env!("CARGO_PKG_VERSION").to_string(),
        rustc_version: command_version("rustc"),
        tools,
    }
}

pub fn print_doctor(info: &PlatformInfo) {
    println!("Tali doctor");
    println!("OS: {}", info.os);
    println!("Architecture: {}", info.architecture);
    println!("Current directory: {}", info.current_directory);
    println!("Shell: {}", info.shell.as_deref().unwrap_or("<unknown>"));
    println!("PATH: {}", info.path.as_deref().unwrap_or("<unknown>"));
    println!("Tali version: {}", info.tali_version);
    println!(
        "Rust binary version: {}",
        info.rustc_version.as_deref().unwrap_or("<missing>")
    );
    println!("Tools:");
    for (name, version) in &info.tools {
        match (version.found, version.version.as_ref()) {
            (true, Some(text)) => println!("- {name}: {text}"),
            (true, None) => println!("- {name}: found"),
            (false, _) => println!("- {name}: missing"),
        }
    }
}

fn common_tools() -> Vec<&'static str> {
    let mut tools = vec![
        "git", "node", "npm", "pnpm", "yarn", "python", "python3", "pip", "cargo", "docker",
    ];
    if cfg!(target_os = "macos") {
        tools.push("brew");
    }
    if cfg!(target_os = "windows") {
        tools.push("winget");
    }
    tools
}

fn capture_tool(name: &str) -> ToolVersion {
    ToolVersion {
        found: command_exists(name),
        version: command_version(name),
    }
}

fn command_exists(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .output()
        .map(|output| {
            output.status.success() || !output.stdout.is_empty() || !output.stderr.is_empty()
        })
        .unwrap_or(false)
}

fn command_version(name: &str) -> Option<String> {
    let output = Command::new(name).arg("--version").output().ok()?;
    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).into_owned()
    } else {
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let first_line = text.lines().next()?.trim().to_string();
    (!first_line.is_empty()).then_some(first_line)
}
