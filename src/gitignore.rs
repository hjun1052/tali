use crate::store::{ManifestSource, Store};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

const RECOMMENDED_IGNORE_BLOCK: &str = "\
# Tali private manifests and runtime files
.tali/
!.tali/share/
!.tali/share/*.toml
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitignoreAction {
    Prompt,
    NoticeOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitignoreOutcome {
    NotProjectManifest,
    SharedManifest,
    NotGitRepository,
    AlreadyIgnored,
    Suppressed,
    NoticePrinted,
    Added,
    Skipped,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PromptCache {
    repositories: BTreeMap<String, PromptCacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PromptCacheEntry {
    dismissed_at: DateTime<Utc>,
}

pub fn maybe_offer_gitignore_protection(
    store: &Store,
    source: &ManifestSource,
    action: GitignoreAction,
) -> Result<GitignoreOutcome> {
    let Some(private_manifest_root) = private_tali_manifest_root(source) else {
        if shared_tali_manifest(source) {
            return Ok(GitignoreOutcome::SharedManifest);
        }
        return Ok(GitignoreOutcome::NotProjectManifest);
    };
    let Some(git_root) = find_git_root(&source.project_root) else {
        return Ok(GitignoreOutcome::NotGitRepository);
    };
    if is_suppressed(store, &git_root)? {
        return Ok(GitignoreOutcome::Suppressed);
    }
    let gitignore_path = git_root.join(".gitignore");
    if tali_is_ignored(&gitignore_path)? {
        return Ok(GitignoreOutcome::AlreadyIgnored);
    }

    match action {
        GitignoreAction::NoticeOnly => {
            print_notice(&private_manifest_root);
            Ok(GitignoreOutcome::NoticePrinted)
        }
        GitignoreAction::Prompt => {
            print_prompt(&private_manifest_root);
            match read_choice()?.as_str() {
                "y" | "yes" => {
                    append_ignore_block(&gitignore_path)?;
                    println!("Added recommended Tali ignore block to:");
                    println!("{}", gitignore_path.display());
                    Ok(GitignoreOutcome::Added)
                }
                "never" => {
                    suppress(store, &git_root)?;
                    println!("Tali will not ask about .tali/ gitignore protection for this repository again.");
                    Ok(GitignoreOutcome::Suppressed)
                }
                _ => Ok(GitignoreOutcome::Skipped),
            }
        }
    }
}

fn private_tali_manifest_root(source: &ManifestSource) -> Option<PathBuf> {
    let relative = source.path.strip_prefix(&source.project_root).ok()?;
    let mut components = relative.components();
    match components.next()? {
        Component::Normal(name) if name == ".tali" => {}
        _ => return None,
    }
    match components.next() {
        Some(Component::Normal(name)) if name == "share" => None,
        _ => Some(source.project_root.join(".tali")),
    }
}

fn shared_tali_manifest(source: &ManifestSource) -> bool {
    let Ok(relative) = source.path.strip_prefix(&source.project_root) else {
        return false;
    };
    let mut components = relative.components();
    matches!(components.next(), Some(Component::Normal(name)) if name == ".tali")
        && matches!(components.next(), Some(Component::Normal(name)) if name == "share")
}

fn find_git_root(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
    }
    None
}

fn tali_is_ignored(gitignore_path: &Path) -> Result<bool> {
    if !gitignore_path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(gitignore_path)
        .with_context(|| format!("failed to read {}", gitignore_path.display()))?;
    Ok(content.lines().any(is_tali_ignore_pattern))
}

fn is_tali_ignore_pattern(line: &str) -> bool {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
        return false;
    }
    matches!(
        line,
        ".tali" | ".tali/" | ".tali/**" | "/.tali" | "/.tali/" | "/.tali/**"
    )
}

fn append_ignore_block(gitignore_path: &Path) -> Result<()> {
    let needs_leading_newline = gitignore_path
        .exists()
        .then(|| fs::read_to_string(gitignore_path))
        .transpose()?
        .is_some_and(|content| !content.is_empty() && !content.ends_with('\n'));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(gitignore_path)
        .with_context(|| format!("failed to open {}", gitignore_path.display()))?;
    if needs_leading_newline {
        writeln!(file)?;
    }
    writeln!(file)?;
    write!(file, "{RECOMMENDED_IGNORE_BLOCK}")?;
    Ok(())
}

fn print_notice(tali_dir: &Path) {
    println!();
    println!("Note: {} is not ignored by git.", tali_dir.display());
    println!("Private Tali manifests may contain local paths or workflow details.");
    println!("To ignore private manifests while allowing shared manifests, add:");
    println!();
    print!("{RECOMMENDED_IGNORE_BLOCK}");
}

fn print_prompt(tali_dir: &Path) {
    println!();
    println!("Tali notice:");
    println!(
        "{} is not ignored by git in this repository.",
        tali_dir.display()
    );
    println!();
    println!("Private Tali manifests may contain local paths or workflow details.");
    print!("Add recommended ignore block to .gitignore? [y/N/never] ");
    let _ = io::stdout().flush();
}

fn read_choice() -> Result<String> {
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().to_ascii_lowercase())
}

fn cache_path(store: &Store) -> PathBuf {
    store.cache_dir().join("gitignore-prompts.json")
}

fn read_cache(store: &Store) -> Result<PromptCache> {
    let path = cache_path(store);
    if !path.exists() {
        return Ok(PromptCache::default());
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(serde_json::from_str(&content).unwrap_or_default())
}

fn write_cache(store: &Store, cache: &PromptCache) -> Result<()> {
    let path = cache_path(store);
    let json = serde_json::to_string_pretty(cache)?;
    fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn is_suppressed(store: &Store, git_root: &Path) -> Result<bool> {
    let cache = read_cache(store)?;
    Ok(cache
        .repositories
        .contains_key(&git_root.to_string_lossy().to_string()))
}

fn suppress(store: &Store, git_root: &Path) -> Result<()> {
    let mut cache = read_cache(store)?;
    cache.repositories.insert(
        git_root.to_string_lossy().to_string(),
        PromptCacheEntry {
            dismissed_at: Utc::now(),
        },
    );
    write_cache(store, &cache)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Manifest;
    use crate::store::ManifestSource;
    use tempfile::tempdir;

    fn source(project_root: &Path, path: PathBuf) -> ManifestSource {
        ManifestSource {
            manifest: Manifest::from_toml(
                r#"
version = 1
name = "setup"

[[steps]]
type = "mkdir"
path = "out"
"#,
            )
            .unwrap(),
            path,
            global: None,
            project_root: project_root.to_path_buf(),
        }
    }

    #[test]
    fn detects_private_tali_manifest() {
        let temp = tempdir().unwrap();
        let source = source(temp.path(), temp.path().join(".tali").join("setup.toml"));

        assert_eq!(
            private_tali_manifest_root(&source),
            Some(temp.path().join(".tali"))
        );
        assert!(!shared_tali_manifest(&source));
    }

    #[test]
    fn skips_shared_tali_manifest() {
        let temp = tempdir().unwrap();
        let source = source(
            temp.path(),
            temp.path().join(".tali").join("share").join("setup.toml"),
        );

        assert_eq!(private_tali_manifest_root(&source), None);
        assert!(shared_tali_manifest(&source));
    }

    #[test]
    fn recognizes_tali_ignore_patterns() {
        assert!(is_tali_ignore_pattern(".tali/"));
        assert!(is_tali_ignore_pattern("/.tali/**"));
        assert!(!is_tali_ignore_pattern("!.tali/share/"));
        assert!(!is_tali_ignore_pattern("# .tali/"));
    }

    #[test]
    fn notice_only_does_not_modify_gitignore() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("data")).unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(project.join(".git")).unwrap();
        fs::create_dir_all(project.join(".tali")).unwrap();
        fs::write(project.join(".tali").join("setup.toml"), "").unwrap();
        let source = source(&project, project.join(".tali").join("setup.toml"));

        let outcome =
            maybe_offer_gitignore_protection(&store, &source, GitignoreAction::NoticeOnly).unwrap();

        assert_eq!(outcome, GitignoreOutcome::NoticePrinted);
        assert!(!project.join(".gitignore").exists());
    }

    #[test]
    fn already_ignored_skips_notice() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("data")).unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(project.join(".git")).unwrap();
        fs::create_dir_all(project.join(".tali")).unwrap();
        fs::write(project.join(".gitignore"), ".tali/\n").unwrap();
        let source = source(&project, project.join(".tali").join("setup.toml"));

        let outcome =
            maybe_offer_gitignore_protection(&store, &source, GitignoreAction::NoticeOnly).unwrap();

        assert_eq!(outcome, GitignoreOutcome::AlreadyIgnored);
    }

    #[test]
    fn appends_recommended_ignore_block() {
        let temp = tempdir().unwrap();
        let gitignore = temp.path().join(".gitignore");
        fs::write(&gitignore, "/target/\n").unwrap();

        append_ignore_block(&gitignore).unwrap();

        let content = fs::read_to_string(&gitignore).unwrap();
        assert!(content.contains(".tali/"));
        assert!(content.contains("!.tali/share/*.toml"));
        assert!(tali_is_ignored(&gitignore).unwrap());
    }

    #[test]
    fn suppresses_repository_after_never_choice() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("data")).unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(&project).unwrap();

        suppress(&store, &project).unwrap();

        assert!(is_suppressed(&store, &project).unwrap());
    }
}
