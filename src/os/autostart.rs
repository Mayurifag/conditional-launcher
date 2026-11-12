use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub(super) fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("Remove {}", path.display())),
    }
}

pub(super) fn write(
    path: &Path,
    content: &[u8],
    permissions: Option<fs::Permissions>,
) -> Result<()> {
    let parent = path.parent().context("Autostart path has no parent")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("Create a temporary file in {}", parent.display()))?;
    file.write_all(content)
        .with_context(|| format!("Write {}", path.display()))?;
    if let Some(permissions) = permissions {
        file.as_file().set_permissions(permissions)?;
    }
    file.as_file().sync_all()?;
    file.persist(path)
        .with_context(|| format!("Replace {}", path.display()))?;
    Ok(())
}

pub(super) fn manage(original: &Path, backup: &Path, placeholder: &[u8]) -> Result<()> {
    let is_symlink = fs::symlink_metadata(original)
        .with_context(|| format!("Read {}", original.display()))?
        .file_type()
        .is_symlink();
    let permissions = fs::metadata(original)
        .with_context(|| format!("Read permissions for {}", original.display()))?
        .permissions();
    if let Some(parent) = backup.parent() {
        fs::create_dir_all(parent).with_context(|| format!("Create {}", parent.display()))?;
    }
    // Reserve the backup without overwriting it and ensure restore can rename it across paths.
    fs::hard_link(original, backup)
        .with_context(|| format!("Create backup {}", backup.display()))?;
    let result = (|| {
        if !is_symlink {
            let content =
                fs::read(original).with_context(|| format!("Read {}", original.display()))?;
            write(backup, &content, Some(permissions.clone()))?;
        }
        write(original, placeholder, Some(permissions))
    })();
    if let Err(error) = result {
        remove(backup).with_context(|| format!("Remove backup after: {error:#}"))?;
        return Err(error);
    }
    Ok(())
}

pub(super) fn restore(original: &Path, backup: &Path) -> Result<()> {
    fs::rename(backup, original)
        .with_context(|| format!("Restore {} from {}", original.display(), backup.display()))
}
