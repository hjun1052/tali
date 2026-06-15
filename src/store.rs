use crate::manifest::Manifest;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use directories::BaseDirs;
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Store {
    data_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ManifestSource {
    pub manifest: Manifest,
    pub path: PathBuf,
    pub global: Option<StoredManifest>,
    pub project_root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredManifest {
    pub id: String,
    pub long_id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub filename: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StoreIndex {
    manifests: Vec<StoredManifest>,
}

impl Store {
    pub fn new() -> Result<Self> {
        if let Ok(path) = env::var("TALI_DATA_DIR") {
            return Self::from_data_dir(PathBuf::from(path));
        }
        let base = BaseDirs::new().context("could not determine platform data directory")?;
        Self::from_data_dir(base.data_dir().join("tali"))
    }

    pub fn from_data_dir(data_dir: PathBuf) -> Result<Self> {
        let store = Store { data_dir };
        store.ensure_layout()?;
        Ok(store)
    }

    pub fn manifests_dir(&self) -> PathBuf {
        self.data_dir.join("manifests")
    }

    pub fn runs_dir(&self) -> PathBuf {
        self.data_dir.join("runs")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache")
    }

    pub fn secrets_dir(&self) -> PathBuf {
        self.data_dir.join("secrets")
    }

    pub fn add_manifest(&self, path: &Path) -> Result<StoredManifest> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read manifest {}", path.display()))?;
        let manifest = Manifest::from_toml(&content)?;
        let mut index = self.read_index()?;
        if index
            .manifests
            .iter()
            .any(|entry| entry.name == manifest.name)
        {
            bail!("a global manifest named '{}' already exists", manifest.name);
        }
        let id = allocate_next_id(index.manifests.iter().map(|entry| entry.id.as_str()));
        let filename = format!("{}-{}.toml", id, sanitize_filename(&manifest.name));
        let dest = self.manifests_dir().join(&filename);
        fs::write(&dest, content).with_context(|| format!("failed to write {}", dest.display()))?;

        let entry = StoredManifest {
            id,
            long_id: generate_long_id(),
            name: manifest.name,
            description: manifest.description,
            created_at: Utc::now(),
            filename,
        };
        index.manifests.push(entry.clone());
        self.write_index(&index)?;
        Ok(entry)
    }

    pub fn list_manifests(&self) -> Result<Vec<StoredManifest>> {
        let mut manifests = self.read_index()?.manifests;
        manifests.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(manifests)
    }

    pub fn resolve_manifest(&self, selector: &str, cwd: &Path) -> Result<ManifestSource> {
        let index = self.read_index()?;
        if let Some(entry) = index
            .manifests
            .iter()
            .find(|entry| entry.id == selector || entry.name == selector)
        {
            let path = self.manifests_dir().join(&entry.filename);
            let manifest = Manifest::from_path(&path)?;
            return Ok(ManifestSource {
                manifest,
                path,
                global: Some(entry.clone()),
                project_root: cwd.to_path_buf(),
            });
        }

        if let Some((project_root, project_manifest)) = find_project_manifest(cwd, selector) {
            let manifest = Manifest::from_path(&project_manifest)?;
            return Ok(ManifestSource {
                manifest,
                path: project_manifest,
                global: None,
                project_root,
            });
        }

        bail!("no manifest found for '{selector}'");
    }

    pub fn create_run_dir(&self, run_id: &str) -> Result<PathBuf> {
        let run_dir = self.runs_dir().join(run_id);
        fs::create_dir_all(&run_dir)
            .with_context(|| format!("failed to create {}", run_dir.display()))?;
        Ok(run_dir)
    }

    pub fn set_latest_run(&self, run_id: &str) -> Result<()> {
        fs::write(self.logs_dir().join("latest"), run_id)?;
        Ok(())
    }

    pub fn set_latest_running_run(&self, run_id: &str) -> Result<()> {
        fs::write(self.logs_dir().join("latest-running"), run_id)?;
        Ok(())
    }

    pub fn clear_latest_running_run(&self, run_id: &str) -> Result<()> {
        let path = self.logs_dir().join("latest-running");
        if !path.exists() {
            return Ok(());
        }
        let current = fs::read_to_string(&path)
            .with_context(|| format!("failed to read latest running pointer {}", path.display()))?;
        if current.trim() == run_id {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
        Ok(())
    }

    pub fn latest_run_id(&self) -> Result<String> {
        let path = self.logs_dir().join("latest");
        let run_id = fs::read_to_string(&path)
            .with_context(|| format!("failed to read latest run pointer {}", path.display()))?;
        Ok(run_id.trim().to_string())
    }

    pub fn latest_running_run_id(&self) -> Result<Option<String>> {
        let path = self.logs_dir().join("latest-running");
        if !path.exists() {
            return Ok(None);
        }
        let run_id = fs::read_to_string(&path)
            .with_context(|| format!("failed to read latest running pointer {}", path.display()))?;
        let run_id = run_id.trim();
        Ok((!run_id.is_empty()).then(|| run_id.to_string()))
    }

    pub fn run_dir(&self, run_id: &str) -> PathBuf {
        self.runs_dir().join(run_id)
    }

    fn ensure_layout(&self) -> Result<()> {
        for dir in [
            self.manifests_dir(),
            self.runs_dir(),
            self.logs_dir(),
            self.cache_dir(),
            self.secrets_dir(),
        ] {
            fs::create_dir_all(&dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }
        Ok(())
    }

    fn index_path(&self) -> PathBuf {
        self.manifests_dir().join("index.json")
    }

    fn read_index(&self) -> Result<StoreIndex> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(StoreIndex::default());
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        Ok(serde_json::from_str(&content)?)
    }

    fn write_index(&self, index: &StoreIndex) -> Result<()> {
        let path = self.index_path();
        let json = serde_json::to_string_pretty(index)?;
        fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}

pub fn allocate_next_id<'a>(existing: impl Iterator<Item = &'a str>) -> String {
    let max = existing
        .filter_map(|id| id.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("{:02}", max + 1)
}

fn generate_long_id() -> String {
    let random: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect();
    format!("{}-{random}", Utc::now().format("%Y%m%d%H%M%S"))
}

fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    sanitized.trim_matches('-').to_string()
}

fn find_project_manifest(cwd: &Path, selector: &str) -> Option<(PathBuf, PathBuf)> {
    for dir in cwd.ancestors() {
        for manifest in [
            dir.join(".tali").join(format!("{selector}.toml")),
            dir.join(".tali")
                .join("share")
                .join(format!("{selector}.toml")),
        ] {
            if manifest.exists() {
                return Some((dir.to_path_buf(), manifest));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn allocates_numeric_ids() {
        assert_eq!(allocate_next_id(["01", "02"].into_iter()), "03");
        assert_eq!(allocate_next_id(std::iter::empty()), "01");
    }

    #[test]
    fn resolves_by_id_and_name() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();
        let manifest_path = temp.path().join("setup.toml");
        fs::write(
            &manifest_path,
            r#"
version = 1
name = "setup"

[[steps]]
type = "mkdir"
path = "config"
"#,
        )
        .unwrap();
        let entry = store.add_manifest(&manifest_path).unwrap();
        assert_eq!(entry.id, "01");

        let by_id = store.resolve_manifest("01", temp.path()).unwrap();
        assert_eq!(by_id.manifest.name, "setup");
        let by_name = store.resolve_manifest("setup", temp.path()).unwrap();
        assert_eq!(by_name.global.unwrap().id, "01");
    }

    #[test]
    fn resolves_project_manifest_from_child_directory() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();
        let project = temp.path().join("project");
        let child = project.join("nested").join("child");
        fs::create_dir_all(project.join(".tali")).unwrap();
        fs::create_dir_all(&child).unwrap();
        fs::write(
            project.join(".tali").join("setup.toml"),
            r#"
version = 1
name = "project-setup"

[[steps]]
type = "mkdir"
path = "config"
"#,
        )
        .unwrap();

        let source = store.resolve_manifest("setup", &child).unwrap();
        assert_eq!(source.manifest.name, "project-setup");
        assert_eq!(source.project_root, project);
    }

    #[test]
    fn resolves_shared_project_manifest_when_private_manifest_is_absent() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();
        let project = temp.path().join("project");
        let child = project.join("nested");
        fs::create_dir_all(project.join(".tali").join("share")).unwrap();
        fs::create_dir_all(&child).unwrap();
        fs::write(
            project.join(".tali").join("share").join("setup.toml"),
            r#"
version = 1
name = "shared-setup"

[[steps]]
type = "mkdir"
path = "config"
"#,
        )
        .unwrap();

        let source = store.resolve_manifest("setup", &child).unwrap();

        assert_eq!(source.manifest.name, "shared-setup");
        assert_eq!(
            source.path,
            project.join(".tali").join("share").join("setup.toml")
        );
        assert_eq!(source.project_root, project);
    }

    #[test]
    fn private_project_manifest_takes_precedence_over_shared_manifest() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(project.join(".tali").join("share")).unwrap();
        fs::write(
            project.join(".tali").join("setup.toml"),
            r#"
version = 1
name = "private-setup"

[[steps]]
type = "mkdir"
path = "private"
"#,
        )
        .unwrap();
        fs::write(
            project.join(".tali").join("share").join("setup.toml"),
            r#"
version = 1
name = "shared-setup"

[[steps]]
type = "mkdir"
path = "shared"
"#,
        )
        .unwrap();

        let source = store.resolve_manifest("setup", &project).unwrap();

        assert_eq!(source.manifest.name, "private-setup");
        assert_eq!(source.path, project.join(".tali").join("setup.toml"));
    }

    #[test]
    fn rejects_duplicate_global_manifest_names() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();
        let first = temp.path().join("first.toml");
        let second = temp.path().join("second.toml");
        let manifest = r#"
version = 1
name = "setup"

[[steps]]
type = "mkdir"
path = "config"
"#;
        fs::write(&first, manifest).unwrap();
        fs::write(&second, manifest).unwrap();

        store.add_manifest(&first).unwrap();
        let error = store.add_manifest(&second).unwrap_err();

        assert!(error.to_string().contains("already exists"));
    }
}
