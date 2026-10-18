//! Çöp Kutusu adaptörleri. macOS'ta çöpe atılan öğenin yeni yolunu bulur ki geri alınabilsin.
use crate::{Error, Result};
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
pub fn trash_item(path: &Path) -> Result<Option<PathBuf>> {
    use trash::macos::{DeleteMethod, TrashContextExtMacos};
    let trash_dir = dirs::home_dir()
        .ok_or_else(|| Error::Trash("no home folder".into()))?
        .join(".Trash");
    let before: std::collections::HashSet<_> = std::fs::read_dir(&trash_dir)?
        .flatten()
        .map(|e| e.path())
        .collect();
    let mut ctx = trash::TrashContext::default();
    ctx.set_delete_method(DeleteMethod::NsFileManager);
    ctx.delete(path).map_err(|e| Error::Trash(e.to_string()))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Yeni giren tek öğe; adı çakışma nedeniyle sonek alabilir.
    let new: Vec<PathBuf> = std::fs::read_dir(&trash_dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| !before.contains(p))
        .collect();
    Ok(new
        .iter()
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(&name))
        })
        .cloned()
        .or_else(|| new.into_iter().next()))
}

#[cfg(not(target_os = "macos"))]
pub fn trash_item(path: &Path) -> Result<Option<PathBuf>> {
    trash::delete(path).map_err(|e| Error::Trash(e.to_string()))?;
    Ok(None)
}

#[cfg(target_os = "macos")]
pub fn restore_item(original: &Path, trashed: Option<&Path>) -> Result<()> {
    // `exists()` symlink'i izler; kırık bir symlink'in üzerine yazmamak için lstat kullanılır.
    let present = |p: &Path| p.symlink_metadata().is_ok();
    let t = trashed.ok_or_else(|| Error::NotFound("trash location was not recorded".into()))?;
    if !present(t) {
        // Önceki bir geri alma dosyayı taşıdı ama günlüğe yazamadıysa: iş zaten bitmiş.
        return if present(original) {
            Ok(())
        } else {
            Err(Error::NotFound(format!(
                "{} is no longer in the Trash (it may have been emptied)",
                t.display()
            )))
        };
    }
    if present(original) {
        return Err(Error::Guard(format!(
            "{} already exists; not overwriting",
            original.display()
        )));
    }
    if let Some(parent) = original.parent() {
        // Günlükteki yol, taşıma anında çözümlenmiş (symlink'siz) halde kaydedilir. Zincirde bugün bir symlink
        // varsa (kökteki /var, /tmp gibi sistem takma adları hariç), dosya ilgisiz bir yere gitmesin.
        for a in parent.ancestors() {
            let root_level = a.parent().is_none_or(|p| p.parent().is_none());
            if !root_level
                && a.symlink_metadata()
                    .is_ok_and(|m| m.file_type().is_symlink())
            {
                return Err(Error::Guard(format!(
                    "{} is now a symlink; not restoring through it",
                    a.display()
                )));
            }
        }
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(t, original)?;
    Ok(())
}

#[cfg(all(
    not(target_os = "macos"),
    any(
        target_os = "windows",
        all(unix, not(target_os = "ios"), not(target_os = "android"))
    )
))]
pub fn restore_item(original: &Path, _trashed: Option<&Path>) -> Result<()> {
    let items = trash::os_limited::list().map_err(|e| Error::Trash(e.to_string()))?;
    let hits: Vec<_> = items
        .into_iter()
        .filter(|i| i.original_path() == original)
        .collect();
    if hits.is_empty() {
        return Err(Error::NotFound(format!(
            "{} is not in the Trash",
            original.display()
        )));
    }
    trash::os_limited::restore_all(hits).map_err(|e| Error::Trash(e.to_string()))
}
