pub mod agents;
pub mod editor;
pub mod marketplace;
mod pins;
#[cfg(windows)]
mod windows;
use editor::{entry_key, entry_uri, read_json, Editor};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex, Weak},
};
use tezbar_extension_sdk::{
    no_view, unchanged, view, BoxFuture, CoreResult, Extension, ExtensionContext,
};
const COMMANDS: &[&str] = &[
    "index",
    "open-with-vscode",
    "open-new-window",
    "open-agents-window",
    "extensions",
    "install-extension",
    "commandpalette",
];
#[derive(Clone)]
enum Action {
    Reload,
    Settings,
    Open(String, bool, bool),
    OpenWith(String, String),
    UnpinAll,
    Copy(String),
    Reveal(String),
    Pin(Value),
    Unpin(String),
    Move(String, i32),
    Remove(String),
    RemoveAll,
    ConfirmRemoveAll,
    Install(String),
    Uninstall(String),
    ConfirmUninstall(String),
    Update(String),
    Extension(String),
    Web(String),
    Quicklink(String, String),
    Command(String),
    Filter,
}
struct Model {
    command: String,
    editor: Option<Editor>,
    prefs: Value,
    query: String,
    filter: String,
    rows: Vec<Value>,
    pins: Vec<Value>,
    hidden: HashSet<String>,
    database_keys: HashSet<String>,
    branches: HashMap<String, Option<String>>,
    actions: HashMap<String, Action>,
    error: Option<String>,
    notice: Option<String>,
    data: PathBuf,
    epoch: u64,
}
impl Model {
    fn reload_pins(&mut self) -> Result<(), String> {
        let path = self.data.join("pinned-v1.json");
        self.pins = if path.exists() {
            let value = read_json(&path, 4 * 1024 * 1024)?;
            let rows = value
                .as_array()
                .filter(|r| r.len() <= 4096)
                .ok_or("Invalid pinned projects")?;
            if rows.iter().any(|r| entry_key(r).is_none()) {
                return Err("Invalid pinned project entry".into());
            }
            rows.clone()
        } else {
            vec![]
        };
        Ok(())
    }
    fn action(&mut self, actions: &mut Vec<Value>, title: &str, effect: Action) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.actions.insert(id.clone(), effect);
        actions.push(json!({"id":id,"title":title,"kind":"action"}));
        id
    }
    fn save_pins(&self) -> Result<(), String> {
        use std::io::Write;
        std::fs::create_dir_all(&self.data).map_err(|_| "Cannot create extension data")?;
        let mut f = tempfile::NamedTempFile::new_in(&self.data)
            .map_err(|_| "Cannot stage pinned projects")?;
        f.write_all(&serde_json::to_vec(&self.pins).map_err(|_| "Invalid pinned projects")?)
            .map_err(|_| "Cannot save pinned projects")?;
        f.as_file()
            .sync_all()
            .map_err(|_| "Cannot flush pinned projects")?;
        f.persist(self.data.join("pinned-v1.json"))
            .map_err(|_| "Cannot replace pinned projects")?;
        Ok(())
    }
    fn render(&mut self) -> Value {
        self.actions.clear();
        let mut actions = vec![];
        let reload = self.action(&mut actions, "Reload", Action::Reload);
        let settings = self.action(&mut actions, "Extension Preferences", Action::Settings);
        let mut global = vec![reload, settings];
        if self.command == "index" {
            if !self.pins.is_empty() {
                global.push(self.action(&mut actions, "Unpin All Projects", Action::UnpinAll));
            }
            global.push(self.action(
                &mut actions,
                "Remove All Recent Projects…",
                Action::RemoveAll,
            ));
        }
        let mut children = vec![];
        if self.command == "index" {
            let pinned: HashSet<_> = self.pins.iter().filter_map(entry_key).collect();
            let all: Vec<_> = self
                .pins
                .iter()
                .cloned()
                .map(|r| (r, true))
                .chain(
                    self.rows
                        .iter()
                        .filter(|r| {
                            entry_key(r)
                                .is_some_and(|k| !pinned.contains(&k) && !self.hidden.contains(&k))
                        })
                        .cloned()
                        .map(|r| (r, false)),
                )
                .collect();
            for (row, pin) in all {
                let Some(uri) = entry_uri(&row).map(str::to_owned) else {
                    continue;
                };
                let Some(key) = entry_key(&row) else { continue };
                let kind = if row.get("workspace").is_some() {
                    if uri.starts_with("vscode-remote:") {
                        "Remote Workspace"
                    } else {
                        "Workspaces"
                    }
                } else if row.get("fileUri").is_some() {
                    "Files"
                } else if uri.starts_with("vscode-remote:") {
                    "Remote Folders"
                } else {
                    "Folders"
                };
                if !self.filter.is_empty() && self.filter != "All Types" && self.filter != kind {
                    continue;
                }
                if uri.starts_with("file:")
                    && !local_path(&uri).is_some_and(|p| std::path::Path::new(&p).exists())
                {
                    continue;
                }
                let name = local_path(&uri)
                    .and_then(|p| {
                        std::path::Path::new(&p)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                    })
                    .unwrap_or_else(|| {
                        url::Url::parse(&uri)
                            .ok()
                            .and_then(|u| {
                                u.path_segments()
                                    .and_then(|mut p| p.next_back().map(str::to_owned))
                            })
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| uri.clone())
                    });
                let default_reuse = self.prefs["closeOtherWindows"].as_bool().unwrap_or(false);
                let mut ids = vec![
                    self.action(
                        &mut actions,
                        if default_reuse {
                            "Open and Close Other Windows"
                        } else {
                            "Open Project"
                        },
                        Action::Open(
                            uri.clone(),
                            default_reuse,
                            row.get("workspace").is_some() || row.get("fileUri").is_some(),
                        ),
                    ),
                    self.action(
                        &mut actions,
                        if default_reuse {
                            "Open Project"
                        } else {
                            "Open and Close Other Windows"
                        },
                        Action::Open(
                            uri.clone(),
                            !default_reuse,
                            row.get("workspace").is_some() || row.get("fileUri").is_some(),
                        ),
                    ),
                    self.action(
                        &mut actions,
                        "Copy Path",
                        Action::Copy(local_path(&uri).unwrap_or_else(|| uri.clone())),
                    ),
                ];
                ids.push(self.action(&mut actions, "Copy Name", Action::Copy(name.clone())));
                if row.get("folderUri").is_some() && uri.starts_with("file:") {
                    if let Some(app) = self.prefs["terminalApp"]["path"]
                        .as_str()
                        .or_else(|| self.prefs["terminalApp"].as_str())
                        .map(str::to_owned)
                    {
                        ids.push(self.action(
                            &mut actions,
                            "Open in Terminal",
                            Action::OpenWith(local_path(&uri).unwrap_or_default(), app),
                        ));
                    }
                }
                if uri.starts_with("file:") {
                    ids.push(self.action(
                        &mut actions,
                        "Reveal Project",
                        Action::Reveal(uri.clone()),
                    ));
                }
                if pin {
                    ids.push(self.action(&mut actions, "Unpin", Action::Unpin(key.clone())));
                    let index = self
                        .pins
                        .iter()
                        .position(|p| entry_key(p).as_deref() == Some(&key))
                        .unwrap_or(0);
                    let grid = self.prefs["layout"] == "grid";
                    if !grid && index > 0 || grid && index >= 6 && index % 6 == 0 {
                        ids.push(self.action(
                            &mut actions,
                            "Move Up",
                            Action::Move(key.clone(), -1),
                        ));
                    }
                    if !grid && index + 1 < self.pins.len()
                        || grid && index < (self.pins.len() / 6) * 6 && (index + 1) % 6 == 0
                    {
                        ids.push(self.action(
                            &mut actions,
                            "Move Down",
                            Action::Move(key.clone(), 1),
                        ));
                    }
                    if grid && index > 0 {
                        ids.push(self.action(
                            &mut actions,
                            "Move Left",
                            Action::Move(key.clone(), -1),
                        ));
                    }
                    if grid && index + 1 < self.pins.len() {
                        ids.push(self.action(
                            &mut actions,
                            "Move Right",
                            Action::Move(key.clone(), 1),
                        ));
                    }
                } else {
                    ids.push(self.action(&mut actions, "Pin", Action::Pin(row.clone())));
                }
                ids.push(self.action(
                    &mut actions,
                    "Remove from Recent Projects",
                    Action::Remove(key.clone()),
                ));
                let deep = self
                    .editor
                    .as_ref()
                    .map(|e| project_uri(e, &uri))
                    .unwrap_or_default();
                ids.push(self.action(
                    &mut actions,
                    "Create Quicklink",
                    Action::Quicklink(name.clone(), deep),
                ));
                let title = if pin { format!("★ {name}") } else { name };
                let mut item = json!({"type":"List.Item","props":{"id":key,"title":title,"subtitle":row["label"].as_str().unwrap_or(&uri),"keywords":[kind,uri.as_str()],"actionIds":ids}});
                if self.prefs["showGitBranch"].as_bool().unwrap_or(true) && uri.starts_with("file:")
                {
                    let branch = self.branches.entry(uri.clone()).or_insert_with(|| {
                        local_path(&uri).and_then(|p| git_branch(std::path::Path::new(&p)))
                    });
                    if let Some(branch) = branch {
                        let color = self.prefs["gitBranchColor"]
                            .as_str()
                            .filter(|c| {
                                c.len() == 7
                                    && c.starts_with('#')
                                    && c.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
                            })
                            .unwrap_or("Green");
                        item["props"]["accessories"] = json!([{"tag":{"value":branch,"color":color},"tooltip":format!("Git Branch: {branch}")}]);
                    }
                }
                children.push(item);
            }
        } else if self.command == "commandpalette" {
            for row in self.rows.clone() {
                let Some(command) = row["command"].as_str() else {
                    continue;
                };
                let title = row["title"].as_str().unwrap_or(command);
                let title = if let Some(category) = row["category"].as_str() {
                    format!("{category}: {title}")
                } else {
                    title.into()
                };
                let run = self.action(&mut actions, "Run Command", Action::Command(command.into()));
                let link = self
                    .editor
                    .as_ref()
                    .map(|e| {
                        format!(
                            "{}://tonka3000.raycast/runcommand?cmd={}",
                            e.scheme,
                            tezbar_extension_sdk::encode_component(command)
                        )
                    })
                    .unwrap_or_default();
                let quick = self.action(
                    &mut actions,
                    "Create Quicklink",
                    Action::Quicklink(format!("VSCode - {title}"), link),
                );
                children.push(json!({"type":"List.Item","props":{"id":command,"title":title,"subtitle":command,"actionIds":[run,quick]}}));
            }
            let helper = self.action(
                &mut actions,
                "Install Raycast for VSCode",
                Action::Extension("tonka3000.raycast".into()),
            );
            global.push(helper);
        } else {
            for row in self.rows.clone() {
                let Some(id) = row["id"].as_str().map(str::to_owned) else {
                    continue;
                };
                let mut ids = vec![];
                if self.command == "extensions" || row["_installed"] == true {
                    ids.push(self.action(
                        &mut actions,
                        "Uninstall Extension…",
                        Action::Uninstall(id.clone()),
                    ));
                    ids.push(self.action(
                        &mut actions,
                        "Update Extension",
                        Action::Update(id.clone()),
                    ));
                } else {
                    ids.push(self.action(
                        &mut actions,
                        "Install Extension",
                        Action::Install(id.clone()),
                    ));
                }
                ids.push(self.action(
                    &mut actions,
                    "Open in Editor",
                    Action::Extension(id.clone()),
                ));
                ids.push(self.action(&mut actions, "Open in Browser", Action::Web(id.clone())));
                ids.push(self.action(&mut actions, "Copy Extension ID", Action::Copy(id.clone())));
                children.push(json!({"type":"List.Item","props":{"id":id,"title":row["name"],"subtitle":format!("{} · {}",id,row["version"].as_str().unwrap_or("")),"actionIds":ids,"detail":{"type":"List.Item.Detail","props":{"markdown":row["description"].as_str().unwrap_or("")}}}}));
            }
        }
        let description = [self.error.as_deref(), self.notice.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        children.push(json!({"type":"List.EmptyView","props":{"title":if self.error.is_some(){"Cannot Load Editor Data"}else if self.command=="install-extension"&&self.query.is_empty(){"Search the VS Code Marketplace"}else{"No Results"},"description":description}}));
        let mut props = json!({"actionIds":global,"searchBarPlaceholder":"Search projects, extensions or commands","isLoading":false});
        if self.command == "install-extension" {
            props["searchText"] = json!(self.query);
            props["__hasServerSearch"] = json!(true);
            props["throttle"] = json!(true);
        }
        if self.command == "index" {
            let id = self.action(&mut actions, "Filter Project Type", Action::Filter);
            props["searchBarAccessory"] = json!({"type":"List.Dropdown","props":{"actionId":id,"value":if self.filter.is_empty(){"All Types"}else{&self.filter}},"children":(["All Types","Folders","Files","Workspaces","Remote Folders","Remote Workspace"].iter().map(|v|json!({"type":"List.Dropdown.Item","props":{"value":v,"title":v}})).collect::<Vec<_>>())});
        }
        let grid = self.command == "index" && self.prefs["layout"] == "grid";
        for child in &mut children {
            if child["type"] == "List.Item" {
                if let Some(ids) = child["props"]["actionIds"].as_array_mut() {
                    ids.extend(global.iter().map(|id| json!(id)));
                }
                if grid {
                    child["type"] = json!("Grid.Item");
                    child["props"]["content"] = json!("resources/icon.png");
                }
            } else if grid && child["type"] == "List.EmptyView" {
                child["type"] = json!("Grid.EmptyView");
            }
        }
        if grid {
            props["columns"] = json!(6);
        }
        let mut result = view(
            "Visual Studio Code",
            json!({"type":if grid {"Grid"} else {"List"},"props":props,"children":children}),
            actions,
        );
        if !description.is_empty() {
            result["message"] = json!(description);
        }
        result
    }
}
fn git_branch(path: &std::path::Path) -> Option<String> {
    use std::io::Read;
    fn small(path: &std::path::Path) -> Option<String> {
        let mut bytes = vec![];
        std::fs::File::open(path)
            .ok()?
            .take(4097)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > 4096 {
            return None;
        }
        String::from_utf8(bytes).ok()
    }
    let start = if path.is_file() { path.parent()? } else { path };
    for parent in start.ancestors().take(64) {
        let git = parent.join(".git");
        let git = if git.is_dir() {
            git
        } else if git.is_file() {
            let pointer = small(&git)?;
            let dir = std::path::PathBuf::from(pointer.trim().strip_prefix("gitdir: ")?);
            if dir.is_absolute() {
                dir
            } else {
                parent.join(dir)
            }
        } else {
            continue;
        };
        let head = small(&git.join("HEAD"))?;
        return if let Some(branch) = head.trim().strip_prefix("ref: refs/heads/") {
            Some(branch.into())
        } else if head.trim().len() == 40 && head.trim().bytes().all(|b| b.is_ascii_hexdigit()) {
            Some("HEAD".into())
        } else {
            None
        };
    }
    None
}
fn confirm_action(v: &mut Value, id: &str) {
    if let Some(ids) = v["root"]["props"]["actionIds"].as_array_mut() {
        ids.insert(0, json!(id));
    }
    if let Some(children) = v["root"]["children"].as_array_mut() {
        for child in children {
            if let Some(ids) = child["props"]["actionIds"].as_array_mut() {
                ids.insert(0, json!(id));
            }
        }
    }
}
async fn close_other_windows(editor: &Editor) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows::close_other(editor)?;
    }
    #[cfg(target_os = "macos")]
    {
        let script=format!("tell application \"System Events\"\ntell process \"{}\"\nrepeat with w in (get windows)\ntry\nclick button 1 of w\nend try\nend repeat\nend tell\nend tell",editor.build);
        let mut command = tokio::process::Command::new("/usr/bin/osascript");
        command.args(["-e", &script]).kill_on_drop(true);
        let output = tokio::time::timeout(std::time::Duration::from_secs(10), command.output())
            .await
            .map_err(|_| "Closing editor windows timed out")?
            .map_err(|_| "Cannot close editor windows")?;
        if !output.status.success() {
            return Err("Cannot close editor windows. Check accessibility permission.".into());
        }
    }
    Ok(())
}
fn local_path(uri: &str) -> Option<String> {
    url::Url::parse(uri)
        .ok()?
        .to_file_path()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}
fn project_uri(editor: &Editor, uri: &str) -> String {
    if uri.starts_with("vscode-remote://") {
        return uri.replacen(
            "vscode-remote://",
            &format!("{}://vscode-remote/", editor.scheme),
            1,
        );
    }
    let Ok(source) = url::Url::parse(uri) else {
        return String::new();
    };
    let text = if let Some(host) = source.host_str().filter(|s| !s.is_empty()) {
        format!("{}://file//{host}{}", editor.scheme, source.path())
    } else {
        format!("{}://file{}", editor.scheme, source.path())
    };
    url::Url::parse(&text).map(String::from).unwrap_or_default()
}
#[derive(Default)]
pub struct Port {
    sessions: Mutex<HashMap<String, Weak<Mutex<Model>>>>,
    pin_lane: Mutex<()>,
    editor_override: Option<Editor>,
}
impl Port {
    /// Explicit owned-fixture constructor. Production always uses Default.
    pub fn with_editor(editor: Editor) -> Self {
        Self {
            editor_override: Some(editor),
            ..Self::default()
        }
    }
    async fn load(&self, ctx: &ExtensionContext, state: &Arc<Mutex<Model>>) -> CoreResult {
        let (editor, command, query, epoch) = {
            let mut s = state.lock().unwrap();
            s.actions.clear();
            s.epoch += 1;
            s.error = None;
            s.rows.clear();
            s.branches.clear();
            (
                s.editor.clone(),
                s.command.clone(),
                s.query.clone(),
                s.epoch,
            )
        };
        let result: Result<Vec<Value>, String> = if let Some(editor) = editor {
            match command.as_str() {
                "index" => editor::recent_with_keys(&editor).map(|(rows, keys)| {
                    state.lock().unwrap().database_keys = keys;
                    rows
                }),
                "extensions" => editor.installed(),
                "install-extension" => {
                    let client = reqwest::Client::builder()
                        .https_only(true)
                        .redirect(reqwest::redirect::Policy::none())
                        .connect_timeout(std::time::Duration::from_secs(5))
                        .timeout(std::time::Duration::from_secs(15))
                        .build()
                        .map_err(|_| "Cannot create Marketplace client")?;
                    {
                        let installed: HashSet<_> = editor
                            .installed()?
                            .iter()
                            .filter_map(|r| r["id"].as_str().map(str::to_lowercase))
                            .collect();
                        marketplace::search(&client, marketplace::ENDPOINT, &query)
                            .await
                            .map(|mut rows| {
                                for row in &mut rows {
                                    row["_installed"] = json!(row["id"]
                                        .as_str()
                                        .is_some_and(|id| installed.contains(&id.to_lowercase())));
                                }
                                rows
                            })
                    }
                }
                "commandpalette" => load_commands(ctx, &editor).await,
                _ => Ok(vec![]),
            }
        } else {
            Err("Editor is not installed. Choose a Build in extension preferences.".into())
        };
        let mut s = state.lock().unwrap();
        if s.epoch != epoch {
            return Ok(unchanged());
        }
        match result {
            Ok(rows) => s.rows = rows,
            Err(error) => s.error = Some(error),
        };
        Ok(s.render())
    }
}
async fn load_commands(_ctx: &ExtensionContext, editor: &Editor) -> Result<Vec<Value>, String> {
    let dir = editor.global.join("tonka3000.raycast/transit");
    std::fs::create_dir_all(&dir).map_err(|_| "Cannot create command transit directory")?;
    let response = dir.join(format!("commands-{}.json", uuid::Uuid::new_v4()));
    let request = json!({"command":"writecommands","args":{"filename":response}});
    use std::io::Write;
    let mut file =
        tempfile::NamedTempFile::new_in(&dir).map_err(|_| "Cannot stage command request")?;
    file.write_all(&serde_json::to_vec(&request).unwrap())
        .map_err(|_| "Cannot write command request")?;
    file.persist(dir.join("request.json"))
        .map_err(|_| "Cannot publish command request")?;
    for _ in 0..50 {
        if response.is_file() {
            if let Ok(v) = read_json(&response, 4 * 1024 * 1024) {
                if let Some(rows) = v.as_array() {
                    if rows.len() > 10000 {
                        return Err("Editor command capacity exceeded".into());
                    }
                    let rows = rows.clone();
                    let _ = std::fs::remove_file(&response);
                    return Ok(rows);
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(
        "No response from Raycast for VSCode. Install and enable tonka3000.raycast, then Reload."
            .into(),
    )
}
impl Extension for Port {
    fn id(&self) -> &'static str {
        "com.tezbar.native.visual-studio-code"
    }
    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    fn commands(&self) -> &'static [&'static str] {
        COMMANDS
    }
    fn handle<'a>(
        &'a self,
        ctx: &'a ExtensionContext,
        command: &'a str,
        operation: &'a str,
        payload: &'a Value,
        _staged: &'a mut Value,
    ) -> BoxFuture<'a, CoreResult> {
        Box::pin(async move {
            if !COMMANDS.contains(&command) {
                return Err("Unknown command".into());
            }
            let id = payload["sessionId"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() < 256)
                .ok_or("Invalid session identity")?;
            if operation == "extension:dispose-session" {
                self.sessions.lock().unwrap().remove(id);
                return Ok(json!({}));
            }
            if operation == "extension:run-command" {
                let prefs = payload["preferences"].clone();
                if command == "open-agents-window" {
                    let build = prefs["build"].as_str().unwrap_or("Code");
                    if agents::uses_cli(build) {
                        let editor = self.editor_override.clone().map(Ok)
                            .unwrap_or_else(|| Editor::production(build))?;
                        editor.cli(&["--agents".into()]).await?;
                    } else {
                        let registered = if matches!(build, "Antigravity" | "Cursor") {
                            None
                        } else {
                            let apps = ctx.host("host.apps.list", json!({})).await?;
                            agents::registered_application(build, cfg!(windows), &apps)
                        };
                        let target = if let Some(target) = registered {
                            PathBuf::from(target)
                        } else if let Some(editor) = &self.editor_override {
                            if cfg!(windows) {
                                editor.executable.clone()
                            } else {
                                editor.app.ancestors().nth(3)
                                    .ok_or("Invalid editor application layout")?.to_path_buf()
                            }
                        } else {
                            agents::production_application(build)?
                        };
                        let target = target.to_str().ok_or("Application path is not Unicode")?;
                        ctx.host("host.shell.open", json!({"target":target})).await?;
                    }
                    ctx.host("host.window.visibility", json!({"visible":false})).await?;
                    return Ok(no_view("Opened agents window"));
                }
                let editor = self.editor_override.clone().map(Ok).unwrap_or_else(|| {
                    Editor::production(prefs["build"].as_str().unwrap_or("Code"))
                });
                if ["open-new-window", "open-with-vscode"].contains(&command) {
                    let editor = editor?;
                    let mut args = vec!["--new-window".into()];
                    if command == "open-with-vscode" {
                        let selected = ctx
                            .host("host.selection.read", json!({"kind":"files","includeDirectories":true,"fallbackToCurrentDirectory":true}))
                            .await?;
                        let selected = selected
                            .as_array()
                            .ok_or("Invalid file selection response")?;
                        if selected.is_empty() {
                            return Err(
                                "Select a file or folder in Finder or Explorer first.".into()
                            );
                        }
                        for p in selected.iter().take(128) {
                            let path = p.as_str().ok_or("Invalid selected path")?;
                            args.push(path.into());
                        }
                    }
                    editor.cli(&args).await?;
                    ctx.host("host.window.visibility", json!({"visible":false}))
                        .await?;
                    return Ok(no_view("Opened editor"));
                }
                let mut legacy: Vec<_> = ctx.migration_paths.iter().collect();
                legacy.sort_by(|a, b| a.0.cmp(b.0));
                let loaded = pins::load(
                    &ctx.paths.data_dir,
                    legacy.into_iter().map(|(_, path)| path.as_path()),
                );
                let (pins, pin_notice) = match loaded {
                    Ok(pins) => (pins, None),
                    Err(error) => (vec![], Some(error)),
                };
                let model = Arc::new(Mutex::new(Model {
                    command: command.into(),
                    editor: editor.ok(),
                    prefs,
                    query: payload["searchText"].as_str().unwrap_or("").into(),
                    filter: String::new(),
                    rows: vec![],
                    pins,
                    hidden: HashSet::new(),
                    database_keys: HashSet::new(),
                    branches: HashMap::new(),
                    actions: HashMap::new(),
                    error: None,
                    notice: pin_notice,
                    data: ctx.paths.data_dir.clone(),
                    epoch: 0,
                }));
                {
                    let mut sessions = self.sessions.lock().unwrap();
                    sessions.retain(|_, s| s.strong_count() > 0);
                    if sessions.len() >= 128 || sessions.contains_key(id) {
                        return Err("Session capacity or duplicate identity".into());
                    }
                    sessions.insert(id.into(), Arc::downgrade(&model));
                }
                let lease = model.clone();
                let owner = ctx.clone();
                ctx.spawn(async move {
                    owner.cancelled().await;
                    drop(lease);
                })?;
                return self.load(ctx, &model).await;
            }
            let state = self
                .sessions
                .lock()
                .unwrap()
                .get(id)
                .and_then(Weak::upgrade)
                .ok_or("Unknown session")?;
            if operation == "extension:search-text-changed" {
                if command != "install-extension" {
                    return Ok(unchanged());
                }
                let query = payload["searchText"]
                    .as_str()
                    .ok_or("Search text must be a string")?;
                {
                    let mut s = state.lock().unwrap();
                    s.actions.clear();
                    s.query = query.into();
                }
                return self.load(ctx, &state).await;
            }
            if operation == "extension:invoke-action" {
                let action = {
                    let mut s = state.lock().unwrap();
                    let action = payload["actionId"]
                        .as_str()
                        .and_then(|id| s.actions.remove(id));
                    if action.is_some() {
                        s.actions.clear();
                    }
                    action
                };
                let Some(action) = action else {
                    return Ok(unchanged());
                };
                let editor = state.lock().unwrap().editor.clone();
                match action {
                    Action::Reload => return self.load(ctx, &state).await,
                    Action::Settings => {
                        ctx.host(
                            "host.settings.open",
                            json!({"surface":"settings-extensions"}),
                        )
                        .await?;
                    }
                    Action::OpenWith(path, app) => {
                        ctx.host("host.shell.openWithApp", json!({"path":path,"appPath":app}))
                            .await?;
                    }
                    Action::UnpinAll => {
                        let _lane = self.pin_lane.lock().unwrap();
                        let mut s = state.lock().unwrap();
                        s.pins.clear();
                        s.save_pins()?;
                    }
                    Action::Copy(text) => {
                        tezbar_extension_sdk::copy(ctx, &text).await?;
                    }
                    Action::Reveal(uri) => {
                        ctx.host(
                            "host.shell.reveal",
                            json!({"path":local_path(&uri).ok_or("Project is not local")?}),
                        )
                        .await?;
                    }
                    Action::Quicklink(name, link) => {
                        ctx.host("host.quicklinks.create", json!({"name":name,"link":link}))
                            .await?;
                    }
                    Action::Open(uri, reuse, is_file) => {
                        let editor = editor.ok_or("Editor unavailable")?;
                        if uri.starts_with("vscode-remote:") {
                            #[cfg(windows)]
                            editor
                                .cli(&[
                                    if reuse {
                                        "--reuse-window"
                                    } else {
                                        "--new-window"
                                    }
                                    .into(),
                                    if is_file {
                                        "--file-uri"
                                    } else {
                                        "--folder-uri"
                                    }
                                    .into(),
                                    uri,
                                ])
                                .await?;
                            #[cfg(not(windows))]
                            {
                                let mut target = url::Url::parse(&project_uri(&editor, &uri))
                                    .map_err(|_| "Invalid remote URI")?;
                                let pairs: Vec<_> = target
                                    .query_pairs()
                                    .filter(|(key, _)| key != "windowId")
                                    .map(|(k, v)| (k.into_owned(), v.into_owned()))
                                    .collect();
                                target.set_query(None);
                                for (k, v) in pairs {
                                    target.query_pairs_mut().append_pair(&k, &v);
                                }
                                if !reuse {
                                    target.query_pairs_mut().append_pair("windowId", "_blank");
                                }
                                tezbar_extension_sdk::open(ctx, target.as_str()).await?;
                            }
                        } else {
                            if reuse {
                                close_other_windows(&editor).await?;
                            }
                            editor
                                .cli(&[
                                    "--new-window".into(),
                                    local_path(&uri).ok_or("Invalid project file URI")?,
                                ])
                                .await?;
                        }
                    }
                    Action::Pin(row) => {
                        let _lane = self.pin_lane.lock().unwrap();
                        let mut s = state.lock().unwrap();
                        s.reload_pins()?;
                        if s.pins.len() >= 4096 {
                            return Err("Pinned project capacity reached".into());
                        }
                        let key = entry_key(&row);
                        s.pins.retain(|r| entry_key(r) != key);
                        s.pins.insert(0, row);
                        s.save_pins()?;
                    }
                    Action::Unpin(key) => {
                        let _lane = self.pin_lane.lock().unwrap();
                        let mut s = state.lock().unwrap();
                        s.reload_pins()?;
                        s.pins.retain(|r| entry_key(r).as_deref() != Some(&key));
                        s.save_pins()?;
                    }
                    Action::Move(key, delta) => {
                        let _lane = self.pin_lane.lock().unwrap();
                        let mut s = state.lock().unwrap();
                        s.reload_pins()?;
                        if let Some(index) = s
                            .pins
                            .iter()
                            .position(|r| entry_key(r).as_deref() == Some(&key))
                        {
                            let new =
                                (index as i32 + delta).clamp(0, s.pins.len() as i32 - 1) as usize;
                            s.pins.swap(index, new);
                            s.save_pins()?;
                        }
                    }
                    Action::Remove(key) => {
                        let is_database = state.lock().unwrap().database_keys.contains(&key);
                        if is_database {
                            editor::remove_recent(
                                &editor.ok_or("Editor unavailable")?,
                                Some(&key),
                            )?;
                        }
                        {
                            let mut s = state.lock().unwrap();
                            s.hidden.insert(key);
                            s.notice=Some("Removed from recent projects. Storage-backed copies stay hidden for this session.".into());
                        }
                        if is_database {
                            return self.load(ctx, &state).await;
                        }
                    }
                    Action::RemoveAll => {
                        let mut s = state.lock().unwrap();
                        let mut v = s.render();
                        let id = s.action(
                            v["actions"].as_array_mut().unwrap(),
                            "Confirm Remove All Recent Projects",
                            Action::ConfirmRemoveAll,
                        );
                        confirm_action(&mut v, &id);
                        v["message"]=json!("Remove all recent projects from the editor history? Project files are preserved.");
                        return Ok(v);
                    }
                    Action::ConfirmRemoveAll => {
                        let is_database = !state.lock().unwrap().database_keys.is_empty();
                        if is_database {
                            editor::remove_recent(&editor.ok_or("Editor unavailable")?, None)?;
                        }
                        {
                            let mut s = state.lock().unwrap();
                            let keys: Vec<_> = s.rows.iter().filter_map(entry_key).collect();
                            s.hidden.extend(keys);
                            s.notice = Some(
                                "All recent projects removed. Project files are preserved.".into(),
                            );
                        }
                        if is_database {
                            return self.load(ctx, &state).await;
                        }
                    }
                    Action::Filter => {
                        let filter = payload["formValues"]["value"]
                            .as_str()
                            .ok_or("Missing project filter")?;
                        if ![
                            "All Types",
                            "Folders",
                            "Files",
                            "Workspaces",
                            "Remote Folders",
                            "Remote Workspace",
                        ]
                        .contains(&filter)
                        {
                            return Err("Invalid project filter".into());
                        }
                        state.lock().unwrap().filter = filter.into();
                    }
                    Action::Uninstall(id) => {
                        let mut s = state.lock().unwrap();
                        let mut v = s.render();
                        let token = s.action(
                            v["actions"].as_array_mut().unwrap(),
                            "Confirm Uninstall Extension",
                            Action::ConfirmUninstall(id.clone()),
                        );
                        confirm_action(&mut v, &token);
                        v["message"] = json!(format!("Uninstall {id}?"));
                        return Ok(v);
                    }
                    Action::Install(id) | Action::Update(id) => {
                        let editor = editor.ok_or("Editor unavailable")?;
                        editor
                            .cli(&["--install-extension".into(), id, "--force".into()])
                            .await?;
                        return self.load(ctx, &state).await;
                    }
                    Action::ConfirmUninstall(id) => {
                        let editor = editor.ok_or("Editor unavailable")?;
                        editor.cli(&["--uninstall-extension".into(), id]).await?;
                        return self.load(ctx, &state).await;
                    }
                    Action::Extension(id) => {
                        let editor = editor.ok_or("Editor unavailable")?;
                        tezbar_extension_sdk::open(
                            ctx,
                            &format!("{}:extension/{id}", editor.scheme),
                        )
                        .await?;
                    }
                    Action::Web(id) => {
                        tezbar_extension_sdk::open(
                            ctx,
                            &format!("https://marketplace.visualstudio.com/items?itemName={id}"),
                        )
                        .await?;
                    }
                    Action::Command(command) => {
                        let editor = editor.ok_or("Editor unavailable")?;
                        tezbar_extension_sdk::open(
                            ctx,
                            &format!(
                                "{}://tonka3000.raycast/runcommand?cmd={}",
                                editor.scheme,
                                tezbar_extension_sdk::encode_component(&command)
                            ),
                        )
                        .await?;
                    }
                }
            } else if !["extension:refresh-session", "extension:load-more"].contains(&operation) {
                return Err("Unsupported operation".into());
            }
            let result = state.lock().unwrap().render();
            Ok(result)
        })
    }
}
