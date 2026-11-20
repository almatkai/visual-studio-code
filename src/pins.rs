//! Import the original useCachedState("pinned") cache without changing it.
use serde_json::Value;
use std::{io::Write, path::Path};
const LIMIT: u64 = 4 * 1024 * 1024;
fn read(path: &Path) -> Result<Value, String> {
    let m = std::fs::symlink_metadata(path).map_err(|_| "Cannot read pinned project storage")?;
    if !m.is_file() || m.file_type().is_symlink() || m.len() > LIMIT {
        return Err("Invalid pinned project storage; original data preserved".into());
    }
    crate::editor::read_json(path, LIMIT)
}
fn entries(value: Value) -> Result<Vec<Value>, String> {
    let rows = value
        .as_array()
        .filter(|v| v.len() <= 4096)
        .ok_or("Invalid pinned project list")?;
    if rows.iter().any(|v| {
        crate::editor::entry_key(v).is_none()
            || !crate::editor::entry_uri(v).is_some_and(|s| {
                s.len() <= 8192
                    && url::Url::parse(s)
                        .is_ok_and(|u| matches!(u.scheme(), "file" | "vscode-remote"))
            })
    }) {
        return Err("Invalid pinned project entry; original data preserved".into());
    }
    Ok(rows.clone())
}
pub fn load<'a>(
    data: &Path,
    sources: impl IntoIterator<Item = &'a Path>,
) -> Result<Vec<Value>, String> {
    let native = data.join("pinned-v1.json");
    if native
        .try_exists()
        .map_err(|_| "Cannot inspect pinned storage")?
    {
        return entries(read(&native)?);
    }
    for source in sources {
        if !source
            .try_exists()
            .map_err(|_| "Cannot inspect legacy pin cache")?
        {
            continue;
        }
        let cache = read(source)?;
        let Some(raw) = cache.get("pinned") else {
            continue;
        };
        let text = raw.as_str().ok_or("Invalid legacy pinned cache value")?;
        let rows = entries(
            serde_json::from_str(text)
                .map_err(|_| "Invalid legacy pinned JSON; original data preserved")?,
        )?;
        std::fs::create_dir_all(data).map_err(|_| "Cannot create pinned storage")?;
        let mut stage =
            tempfile::NamedTempFile::new_in(data).map_err(|_| "Cannot stage imported pins")?;
        stage
            .write_all(&serde_json::to_vec(&rows).map_err(|_| "Invalid imported pins")?)
            .map_err(|_| "Cannot write imported pins")?;
        stage
            .as_file()
            .sync_all()
            .map_err(|_| "Cannot flush imported pins")?;
        match stage.persist_noclobber(&native) {
            Ok(_) => return Ok(rows),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
                return entries(read(&native)?)
            }
            Err(_) => return Err("Cannot publish imported pins; legacy cache preserved".into()),
        }
    }
    Ok(vec![])
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn imports_real_shim_shape_preserving_order_and_source() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("shared.json");
        let rows = json!([{"folderUri":"file:///C:/owned/a"},{"workspace":{"configPath":"vscode-remote://ssh-remote+owned/custom"},"remoteAuthority":"ssh-remote+owned"}]);
        let bytes =
            serde_json::to_vec(&json!({"pinned":rows.to_string(),"other":"untouched"})).unwrap();
        std::fs::write(&source, &bytes).unwrap();
        assert_eq!(
            load(&t.path().join("data"), [source.as_path()]).unwrap(),
            rows.as_array().unwrap().clone()
        );
        assert_eq!(std::fs::read(&source).unwrap(), bytes);
    }
    #[test]
    fn empty_native_store_prevents_deleted_pins_returning() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("shared.json");
        std::fs::write(
            &source,
            json!({"pinned":"[{\"folderUri\":\"file:///C:/old\"}]"}).to_string(),
        )
        .unwrap();
        std::fs::write(t.path().join("pinned-v1.json"), "[]").unwrap();
        assert!(load(t.path(), [source.as_path()]).unwrap().is_empty());
    }
    #[test]
    fn malformed_legacy_data_does_not_create_native_store() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("shared.json");
        std::fs::write(&source, json!({"pinned":"broken"}).to_string()).unwrap();
        assert!(load(&t.path().join("data"), [source.as_path()]).is_err());
        assert!(!t.path().join("data/pinned-v1.json").exists());
    }
}
