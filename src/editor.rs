use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub const BUILDS: &[(&str, &str, &str, &str)] = &[
    (
        "Antigravity",
        "antigravity",
        "Antigravity",
        "Antigravity.app",
    ),
    (
        "Code",
        "vscode",
        "Microsoft VS Code",
        "Visual Studio Code.app",
    ),
    (
        "Code - Insiders",
        "vscode-insiders",
        "Microsoft VS Code Insiders",
        "Visual Studio Code - Insiders.app",
    ),
    ("Cursor", "cursor", "cursor", "Cursor.app"),
    ("IBM Bob", "bobide", "IBM Bob", "IBM Bob.app"),
    ("Kiro", "kiro", "Kiro", "Kiro.app"),
    ("Positron", "positron", "Positron", "Positron.app"),
    ("Qoder", "qoder", "Qoder", "Qoder.app"),
    ("Trae", "trae", "Trae", "Trae.app"),
    ("Trae CN", "trae-cn", "Trae CN", "Trae CN.app"),
    ("VSCodium", "vscode-oss", "VSCodium", "VSCodium.app"),
    (
        "VSCodium - Insiders",
        "vscode-oss-insiders",
        "VSCodium Insiders",
        "VSCodium - Insiders.app",
    ),
    ("Devin", "devin", "Devin", "Devin.app"),
    ("Windsurf", "windsurf", "Windsurf", "Windsurf.app"),
    ("Lingma", "lingma", "Lingma", "Lingma.app"),
];
pub fn read_json(path: &Path, limit: u64) -> Result<Value, String> {
    use std::io::Read;
    let mut bytes = vec![];
    std::fs::File::open(path)
        .and_then(|f| f.take(limit + 1).read_to_end(&mut bytes))
        .map_err(|_| format!("Cannot read {}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err("JSON file exceeds limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| format!("Invalid JSON in {}", path.display()))
}
#[derive(Clone, Debug)]
pub struct Editor {
    pub build: String,
    pub scheme: String,
    pub home: PathBuf,
    pub global: PathBuf,
    pub app: PathBuf,
    pub executable: PathBuf,
    pub database: PathBuf,
    pub extensions: PathBuf,
}
impl Editor {
    pub fn production(build: &str) -> Result<Self, String> {
        let def = BUILDS
            .iter()
            .find(|b| b.0 == build)
            .ok_or("Unknown editor build")?;
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(PathBuf::from)
            .ok_or("Home directory unavailable")?;
        let support = if cfg!(windows) {
            std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .unwrap_or(home.join("AppData/Roaming"))
        } else {
            home.join("Library/Application Support")
        };
        let mut roots = if cfg!(windows) {
            vec![
                std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .unwrap_or(home.join("AppData/Local"))
                    .join("Programs"),
                PathBuf::from(
                    std::env::var_os("ProgramFiles").unwrap_or_else(|| "C:/Program Files".into()),
                ),
                PathBuf::from(
                    std::env::var_os("ProgramFiles(x86)")
                        .unwrap_or_else(|| "C:/Program Files (x86)".into()),
                ),
            ]
        } else {
            vec![PathBuf::from("/Applications"), home.join("Applications")]
        };
        let folder = if cfg!(windows) { def.2 } else { def.3 };
        let install = roots
            .drain(..)
            .map(|r| r.join(folder))
            .find(|p| p.is_dir())
            .ok_or("Editor is not installed. Choose another Build in extension preferences.")?;
        let (app, executable) = if cfg!(windows) {
            let mut candidates = vec![install.join("resources/app")];
            if let Ok(dirs) = std::fs::read_dir(&install) {
                let mut versioned: Vec<_> = dirs
                    .flatten()
                    .filter(|d| {
                        d.file_name().to_str().is_some_and(|n| {
                            n.len() == 10 && n.bytes().all(|b| b.is_ascii_hexdigit())
                        })
                    })
                    .collect();
                versioned.sort_by_key(|d| {
                    std::cmp::Reverse(d.metadata().and_then(|m| m.modified()).ok())
                });
                candidates.extend(
                    versioned
                        .into_iter()
                        .map(|d| d.path().join("resources/app")),
                );
            }
            let app = candidates
                .into_iter()
                .find(|p| p.join("product.json").is_file())
                .ok_or("Editor product.json unavailable")?;
            let product = read_json(&app.join("product.json"), 2 * 1024 * 1024)?;
            let exe = windows_executable(&install, build, &product)
                .ok_or("Editor executable unavailable")?;
            (app, exe)
        } else {
            let app = install.join("Contents/Resources/app");
            let product = read_json(&app.join("product.json"), 2 * 1024 * 1024)?;
            let exe = install
                .join("Contents/MacOS")
                .join(product["nameShort"].as_str().unwrap_or("Electron"));
            let exe = if exe.is_file() {
                exe
            } else {
                install.join("Contents/MacOS/Electron")
            };
            (app, exe)
        };
        let global = support.join(build).join("User/globalStorage");
        let product = read_json(&app.join("product.json"), 2 * 1024 * 1024)?;
        let database = product["sharedDataFolderName"]
            .as_str()
            .filter(|n| !n.contains(['/', '\\']) && *n != ".." && !n.is_empty())
            .map(|n| home.join(n).join("sharedStorage/state.vscdb"))
            .filter(|p| p.is_file())
            .unwrap_or_else(|| global.join("state.vscdb"));
        let extensions = home.join(format!(".{}/extensions", def.1));
        Ok(Self {
            build: build.into(),
            scheme: def.1.into(),
            home,
            global,
            app,
            executable,
            database,
            extensions,
        })
    }
    pub async fn cli(&self, args: &[String]) -> Result<String, String> {
        use std::process::Stdio;
        use tokio::io::AsyncReadExt;
        let mut command = tokio::process::Command::new(&self.executable);
        command
            .env("ELECTRON_RUN_AS_NODE", "1")
            .arg(self.app.join("out/cli.js"))
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command.spawn().map_err(|_| "Cannot start editor CLI")?;
        let stdout = child.stdout.take().ok_or("No editor stdout")?;
        let stderr = child.stderr.take().ok_or("No editor stderr")?;
        let mut out = vec![];
        let mut err = vec![];
        let work = async {
            tokio::try_join!(
                async {
                    stdout
                        .take(1024 * 1024 + 1)
                        .read_to_end(&mut out)
                        .await
                        .map(|_| ())
                },
                async {
                    stderr
                        .take(1024 * 1024 + 1)
                        .read_to_end(&mut err)
                        .await
                        .map(|_| ())
                },
                async {
                    let status = child.wait().await?;
                    if status.success() {
                        Ok(())
                    } else {
                        Err(std::io::Error::other("Editor CLI failed"))
                    }
                }
            )
        };
        tokio::time::timeout(std::time::Duration::from_secs(25), work)
            .await
            .map_err(|_| "Editor CLI timed out")?
            .map_err(|_| "Editor CLI failed")?;
        if out.len() > 1024 * 1024 || err.len() > 1024 * 1024 {
            return Err("Editor CLI output exceeds limit".into());
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }
    pub fn installed(&self) -> Result<Vec<Value>, String> {
        let index = self.extensions.join("extensions.json");
        if !index.exists() {
            return Ok(vec![]);
        }
        let entries = read_json(&index, 4 * 1024 * 1024)?;
        let mut out = vec![];
        for entry in entries
            .as_array()
            .ok_or("Invalid installed extensions index")?
            .iter()
            .take(4096)
        {
            let id = entry["identifier"]["id"].as_str().unwrap_or("");
            if !extension_id(id) {
                continue;
            }
            let path = installed_location(&self.extensions, entry);
            let pkg = path
                .as_ref()
                .and_then(|path| read_json(&path.join("package.json"), 2 * 1024 * 1024).ok())
                .unwrap_or_default();
            let mut name = pkg["displayName"]
                .as_str()
                .or_else(|| pkg["name"].as_str())
                .unwrap_or(id)
                .to_string();
            if name.starts_with('%') && name.ends_with('%') {
                if let Some(nls) = path
                    .as_ref()
                    .and_then(|path| read_json(&path.join("package.nls.json"), 1024 * 1024).ok())
                {
                    if let Some(text) = nls[name.trim_matches('%')].as_str() {
                        name = text.into();
                    }
                }
            }
            let version = entry["version"]
                .as_str()
                .or_else(|| pkg["version"].as_str());
            out.push(json!({"id":id,"name":name,"version":version,"path":path,"description":pkg["description"],"commands":pkg["contributes"]["commands"]}));
        }
        Ok(out)
    }
}

pub(super) fn windows_executable(install: &Path, build: &str, product: &Value) -> Option<PathBuf> {
    let name = if build == "Trae CN" {
        "Trae - CN"
    } else {
        build
    };
    let product_names = ["nameShort", "applicationName"]
        .into_iter()
        .filter_map(|key| product[key].as_str())
        .filter(|name| {
            !name.trim().is_empty()
                && !name.contains(['/', '\\', ':'])
                && !name.chars().any(char::is_control)
        });
    std::iter::once(name)
        .chain(product_names)
        .map(|name| install.join(format!("{name}.exe")))
        .find(|file| file.is_file())
}

fn installed_location(root: &Path, entry: &Value) -> Option<PathBuf> {
    let relative = entry["relativeLocation"]
        .as_str()
        .filter(|path| !path.is_empty())
        .map(|path| root.join(path));
    if relative.as_ref().is_some_and(|path| path.is_dir()) {
        return relative;
    }
    let location = &entry["location"];
    if let Some(text) = location.as_str().filter(|path| !path.is_empty()) {
        let path = url::Url::parse(text)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .unwrap_or_else(|| root.join(text));
        return Some(path);
    }
    if let Some(path) = location["fsPath"].as_str().filter(|path| !path.is_empty()) {
        return Some(PathBuf::from(path));
    }
    if let Some(path) = location["path"].as_str().filter(|path| !path.is_empty()) {
        if cfg!(windows) {
            if let Some(host) = location["authority"]
                .as_str()
                .filter(|host| !host.is_empty())
            {
                return Some(PathBuf::from(format!(
                    r"\\{host}{}",
                    path.replace('/', "\\")
                )));
            }
            let bytes = path.as_bytes();
            if matches!(bytes, [b'/', drive, b':', b'/', ..] if drive.is_ascii_alphabetic()) {
                return Some(PathBuf::from(&path[1..]));
            }
        }
        return Some(PathBuf::from(path));
    }
    relative
}
pub fn extension_id(id: &str) -> bool {
    id.len() <= 256
        && id.split('.').count() == 2
        && id.split('.').all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}
pub fn entry_uri(v: &Value) -> Option<&str> {
    v["folderUri"]
        .as_str()
        .or_else(|| v["workspace"]["configPath"].as_str())
        .or_else(|| v["fileUri"].as_str())
}
pub fn entry_key(v: &Value) -> Option<String> {
    let kind = if v["folderUri"].is_string() {
        "folder"
    } else if v["workspace"]["configPath"].is_string() {
        "workspace"
    } else {
        "file"
    };
    entry_uri(v).map(|u| format!("{kind}:{u}"))
}

const RECENT_KEYS: [&str; 2] = ["recently.opened", "history.recentlyOpenedPathsList"];

fn open_history(editor: &Editor, flags: rusqlite::OpenFlags) -> Result<Connection, String> {
    let db = Connection::open_with_flags(&editor.database, flags)
        .map_err(|_| "Cannot open recent project database")?;
    db.busy_timeout(std::time::Duration::from_millis(500))
        .map_err(|_| "Database unavailable")?;
    Ok(db)
}

fn active_history(db: &Connection) -> Result<Option<(String, Value)>, String> {
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT key, value FROM ItemTable WHERE key IN (?1, ?2)
             ORDER BY CASE key WHEN ?1 THEN 0 ELSE 1 END LIMIT 1",
            RECENT_KEYS,
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| "Cannot read recent projects")?;
    row.map(|(key, serialized)| {
        if serialized.len() > 16 * 1024 * 1024 {
            return Err("Recent projects exceed limit".into());
        }
        serde_json::from_str(&serialized)
            .map(|value| (key, value))
            .map_err(|_| "Invalid recent project data".into())
    })
    .transpose()
}

fn database_entries(editor: &Editor) -> Result<Vec<Value>, String> {
    if !editor.database.is_file() {
        return Ok(Vec::new());
    }
    let db = open_history(editor, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    Ok(active_history(&db)?
        .and_then(|(_, value)| value["entries"].as_array().cloned())
        .unwrap_or_default())
}

fn browsable_key(entry: &Value) -> Option<String> {
    let uri = entry_uri(entry)?;
    if uri.starts_with("file://") || uri.starts_with("vscode-remote://") {
        entry_key(entry)
    } else {
        None
    }
}

pub fn recent(editor: &Editor) -> Result<Vec<Value>, String> {
    recent_with_keys(editor).map(|(rows, _)| rows)
}

pub fn recent_with_keys(editor: &Editor) -> Result<(Vec<Value>, HashSet<String>), String> {
    let database = database_entries(editor);
    let mut backups = Vec::new();
    if let Ok(storage) = read_json(&editor.global.join("storage.json"), 16 * 1024 * 1024) {
        for key in ["workspaces", "folders"] {
            if let Some(entries) = storage["backupWorkspaces"][key].as_array() {
                backups.extend(entries.clone());
            }
        }
        backups.reverse();
        if let Some(profiles) = storage["profileAssociations"]["workspaces"].as_object() {
            for uri in profiles.keys().rev() {
                backups.push(if uri.ends_with(".code-workspace") {
                    json!({"workspace":{"configPath":uri}})
                } else {
                    json!({"folderUri":uri})
                });
            }
        }
    }
    let mut out = match database {
        Ok(rows) => rows,
        Err(_) if backups.iter().any(|entry| browsable_key(entry).is_some()) => Vec::new(),
        Err(error) => return Err(error),
    };
    let database_keys = out.iter().filter_map(browsable_key).collect();
    out.extend(backups);
    let mut seen = HashSet::new();
    out.retain(|v| browsable_key(v).is_some_and(|key| seen.insert(key)));
    if out.len() > 4096 {
        return Err("Recent project capacity exceeded".into());
    }
    Ok((out, database_keys))
}
pub fn remove_recent(editor: &Editor, entry: Option<&str>) -> Result<(), String> {
    let mut db = open_history(editor, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| "Recent projects are busy")?;
    if let Some(entry) = entry {
        if let Some((key, mut history)) = active_history(&tx)? {
            let rows = history["entries"]
                .as_array_mut()
                .ok_or("Invalid project list")?;
            let count = rows.len();
            rows.retain(|row| entry_key(row).as_deref() != Some(entry));
            if rows.len() == count {
                return Ok(());
            }
            tx.execute(
                "UPDATE ItemTable SET value=?1 WHERE key=?2",
                rusqlite::params![
                    serde_json::to_string(&history).map_err(|_| "Invalid recent projects")?,
                    key
                ],
            )
            .map_err(|_| "Cannot update recent projects")?;
        }
    } else {
        for key in RECENT_KEYS {
            tx.execute(
                "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [key, r#"{"entries":[]}"#],
            )
            .map_err(|_| "Cannot clear recent projects")?;
        }
    }
    tx.commit()
        .map_err(|_| "Cannot save recent projects".to_owned())
}
