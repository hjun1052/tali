use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

const SKILL_MD: &str = include_str!("../.codex/skills/tali-agent/SKILL.md");
const OPENAI_YAML: &str = include_str!("../.codex/skills/tali-agent/agents/openai.yaml");
const MANIFEST_AUTHORING: &str =
    include_str!("../.codex/skills/tali-agent/references/manifest-authoring.md");
const LIVE_LOGS_AND_REPAIR: &str =
    include_str!("../.codex/skills/tali-agent/references/live-logs-and-repair.md");

#[derive(Debug, Clone)]
pub struct SkillInstallResult {
    pub path: PathBuf,
    pub backup_path: Option<PathBuf>,
}

pub fn install_tali_agent_skill(skill_root: &Path, overwrite: bool) -> Result<SkillInstallResult> {
    let target = skill_root.join("tali-agent");
    let backup_path = if target.exists() {
        if !overwrite {
            anyhow::bail!("skill already exists at {}", target.display());
        }
        let backup = target.with_file_name(format!(
            "tali-agent.bak-{}",
            chrono::Utc::now().format("%Y%m%d%H%M%S")
        ));
        if backup.exists() {
            fs::remove_dir_all(&backup)
                .with_context(|| format!("failed to remove {}", backup.display()))?;
        }
        fs::rename(&target, &backup)
            .with_context(|| format!("failed to back up {}", target.display()))?;
        Some(backup)
    } else {
        None
    };

    fs::create_dir_all(target.join("agents"))
        .with_context(|| format!("failed to create {}", target.join("agents").display()))?;
    fs::create_dir_all(target.join("references"))
        .with_context(|| format!("failed to create {}", target.join("references").display()))?;

    write_file(&target.join("SKILL.md"), SKILL_MD)?;
    write_file(&target.join("agents").join("openai.yaml"), OPENAI_YAML)?;
    write_file(
        &target.join("references").join("manifest-authoring.md"),
        MANIFEST_AUTHORING,
    )?;
    write_file(
        &target.join("references").join("live-logs-and-repair.md"),
        LIVE_LOGS_AND_REPAIR,
    )?;

    Ok(SkillInstallResult {
        path: target,
        backup_path,
    })
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}
