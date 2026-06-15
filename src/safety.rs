use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub fn safe_path(base: &Path, requested: &str, allow_outside_cwd: bool) -> Result<PathBuf> {
    let requested_path = Path::new(requested);
    if allow_outside_cwd {
        return Ok(if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            base.join(requested_path)
        });
    }

    if requested_path.is_absolute() {
        bail!("path '{}' is outside the working directory", requested);
    }

    for component in requested_path.components() {
        match component {
            Component::ParentDir => {
                bail!(
                    "path '{}' traverses outside the working directory",
                    requested
                )
            }
            Component::Prefix(_) | Component::RootDir => {
                bail!("path '{}' is outside the working directory", requested)
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    let target = base.join(requested_path);
    ensure_existing_ancestor_inside(base, &target, requested)?;
    Ok(target)
}

pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn ensure_existing_ancestor_inside(base: &Path, target: &Path, requested: &str) -> Result<()> {
    let base = fs::canonicalize(base)
        .with_context(|| format!("failed to canonicalize base directory {}", base.display()))?;
    let existing = nearest_existing_path(target);
    if let Some(existing) = existing {
        let canonical = fs::canonicalize(&existing).with_context(|| {
            format!(
                "failed to canonicalize existing path component {}",
                existing.display()
            )
        })?;
        if !canonical.starts_with(&base) {
            bail!(
                "path '{}' resolves outside the working directory through an existing path component",
                requested
            );
        }
    }
    Ok(())
}

fn nearest_existing_path(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(path) = current {
        if path.exists() {
            return Some(path.to_path_buf());
        }
        current = path.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rejects_path_traversal() {
        let base = Path::new("/tmp/project");
        assert!(safe_path(base, "../secret", false).is_err());
        assert!(safe_path(base, "/tmp/secret", false).is_err());
    }

    #[test]
    fn accepts_normal_project_relative_path() {
        let temp = tempdir().unwrap();
        assert!(safe_path(temp.path(), "config/app.toml", false).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_for_new_child() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        symlink(&outside, project.join("link")).unwrap();

        assert!(safe_path(&project, "link/secret.txt", false).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_existing_symlink_file_escape() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        let outside = temp.path().join("outside.txt");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(&outside, "outside").unwrap();
        symlink(&outside, project.join("secret.txt")).unwrap();

        assert!(safe_path(&project, "secret.txt", false).is_err());
    }
}
