use crate::condition;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub allow_outside_cwd: bool,
    #[serde(default)]
    pub inputs: Vec<InputSpec>,
    #[serde(default)]
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputSpec {
    pub name: String,
    pub prompt: String,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Step {
    #[serde(rename = "shell")]
    Shell {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        when: Option<String>,
        cmd: String,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    #[serde(rename = "write_file")]
    WriteFile {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        when: Option<String>,
        path: String,
        content: String,
        #[serde(default = "default_true")]
        overwrite: bool,
        #[serde(default = "default_true")]
        create_dirs: bool,
    },
    #[serde(rename = "mkdir")]
    Mkdir {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        when: Option<String>,
        path: String,
    },
    #[serde(rename = "copy")]
    Copy {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        when: Option<String>,
        from: String,
        to: String,
        #[serde(default)]
        overwrite: bool,
    },
    #[serde(rename = "replace_in_file")]
    ReplaceInFile {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        when: Option<String>,
        path: String,
        replacements: BTreeMap<String, String>,
        #[serde(default = "default_true")]
        require_match: bool,
        #[serde(default)]
        expected_matches: Option<usize>,
    },
}

fn default_true() -> bool {
    true
}

impl Manifest {
    pub fn from_path(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read manifest {}", path.display()))?;
        Self::from_toml(&content)
            .with_context(|| format!("failed to parse manifest {}", path.display()))
    }

    pub fn from_toml(content: &str) -> Result<Self> {
        let manifest: Manifest = toml::from_str(content)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("unsupported manifest version {}; expected 1", self.version);
        }
        if self.name.trim().is_empty() {
            bail!("manifest name is required");
        }
        if self.steps.is_empty() {
            bail!("manifest must contain at least one step");
        }
        let mut input_names = BTreeSet::new();
        for input in &self.inputs {
            if input.name.trim().is_empty() {
                bail!("input name is required");
            }
            if !is_valid_input_name(&input.name) {
                bail!("invalid input name '{}'", input.name);
            }
            if !input_names.insert(input.name.clone()) {
                bail!("duplicate input name '{}'", input.name);
            }
            if input.prompt.trim().is_empty() {
                bail!("input '{}' prompt is required", input.name);
            }
        }
        for (index, step) in self.steps.iter().enumerate() {
            step.validate(index + 1)?;
        }
        Ok(())
    }
}

impl Step {
    pub fn name(&self) -> Option<&str> {
        match self {
            Step::Shell { name, .. }
            | Step::WriteFile { name, .. }
            | Step::Mkdir { name, .. }
            | Step::Copy { name, .. }
            | Step::ReplaceInFile { name, .. } => name.as_deref(),
        }
    }

    pub fn when(&self) -> Option<&str> {
        match self {
            Step::Shell { when, .. }
            | Step::WriteFile { when, .. }
            | Step::Mkdir { when, .. }
            | Step::Copy { when, .. }
            | Step::ReplaceInFile { when, .. } => when.as_deref(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Step::Shell { .. } => "shell",
            Step::WriteFile { .. } => "write_file",
            Step::Mkdir { .. } => "mkdir",
            Step::Copy { .. } => "copy",
            Step::ReplaceInFile { .. } => "replace_in_file",
        }
    }

    pub fn plan_line(&self) -> String {
        match self {
            Step::Shell { cmd, .. } => format!("Shell: {cmd}"),
            Step::WriteFile { path, .. } => format!("Write file: {path}"),
            Step::Mkdir { path, .. } => format!("Create directory: {path}"),
            Step::Copy { from, to, .. } => format!("Copy: {from} -> {to}"),
            Step::ReplaceInFile { path, .. } => format!("Replace in file: {path}"),
        }
    }

    pub fn command_for_inspect(&self) -> Option<&str> {
        match self {
            Step::Shell { cmd, .. } => Some(cmd),
            _ => None,
        }
    }

    pub fn path_for_inspect(&self) -> Option<&str> {
        match self {
            Step::WriteFile { path, .. }
            | Step::Mkdir { path, .. }
            | Step::ReplaceInFile { path, .. } => Some(path),
            Step::Copy { to, .. } => Some(to),
            Step::Shell { .. } => None,
        }
    }

    fn validate(&self, index: usize) -> Result<()> {
        match self {
            Step::Shell { cmd, .. } if cmd.trim().is_empty() => {
                bail!("step {index} shell cmd is required")
            }
            Step::WriteFile { path, .. } if path.trim().is_empty() => {
                bail!("step {index} write_file path is required")
            }
            Step::Mkdir { path, .. } if path.trim().is_empty() => {
                bail!("step {index} mkdir path is required")
            }
            Step::Copy { from, to, .. } if from.trim().is_empty() || to.trim().is_empty() => {
                bail!("step {index} copy from and to are required")
            }
            Step::ReplaceInFile {
                path, replacements, ..
            } if path.trim().is_empty() || replacements.is_empty() => {
                bail!("step {index} replace_in_file path and replacements are required")
            }
            Step::ReplaceInFile { replacements, .. }
                if replacements
                    .keys()
                    .any(|placeholder| placeholder.is_empty()) =>
            {
                bail!("step {index} replace_in_file replacement keys cannot be empty")
            }
            _ => {
                if let Some(when) = self.when() {
                    if when.trim().is_empty() {
                        bail!("step {index} when condition cannot be empty");
                    }
                    condition::validate_syntax(when)
                        .with_context(|| format!("step {index} has invalid when condition"))?;
                }
                Ok(())
            }
        }
    }
}

fn is_valid_input_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest() {
        let manifest = Manifest::from_toml(
            r#"
version = 1
name = "nextjs-blog"
description = "Install dependencies."

[[inputs]]
name = "database_url"
prompt = "Database URL"
secret = true
required = true

[[steps]]
name = "Install"
type = "shell"
when = "os_is('linux') || os_is('macos')"
cmd = "npm install"
"#,
        )
        .unwrap();

        assert_eq!(manifest.name, "nextjs-blog");
        assert_eq!(manifest.inputs[0].name, "database_url");
        assert_eq!(
            manifest.steps[0].when(),
            Some("os_is('linux') || os_is('macos')")
        );
        assert!(matches!(manifest.steps[0], Step::Shell { .. }));
    }

    #[test]
    fn parses_replace_in_file_step() {
        let manifest = Manifest::from_toml(
            r##"
version = 1
name = "replace-test"

[[steps]]
name = "Fill placeholder"
type = "replace_in_file"
path = ".env"
expected_matches = 1

[steps.replacements]
"__TOKEN__" = "{{token}}"
"#NAME#" = "demo"
"##,
        )
        .unwrap();

        assert!(matches!(
            manifest.steps[0],
            Step::ReplaceInFile {
                expected_matches: Some(1),
                ..
            }
        ));
    }

    #[test]
    fn rejects_duplicate_input_names() {
        let error = Manifest::from_toml(
            r#"
version = 1
name = "duplicate-inputs"

[[inputs]]
name = "project_name"
prompt = "Project name"

[[inputs]]
name = "project_name"
prompt = "Project name again"

[[steps]]
type = "shell"
cmd = "echo ok"
"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("duplicate input name"));
    }

    #[test]
    fn rejects_invalid_when_condition() {
        let error = Manifest::from_toml(
            r#"
version = 1
name = "invalid-when"

[[steps]]
type = "shell"
when = "file_exists(config)"
cmd = "echo ok"
"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("invalid when condition"));
    }
}
