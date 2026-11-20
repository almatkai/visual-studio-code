#[allow(dead_code)]
#[path = "../src/editor.rs"]
mod editor;

use rusqlite::Connection;
use serde_json::{json, Value};
use std::{fs, path::Path};

fn fixture(root: &Path) -> editor::Editor {
    let global = root.join("globalStorage");
    let extensions = root.join("extensions");
    fs::create_dir(&global).unwrap();
    fs::create_dir(&extensions).unwrap();
    editor::Editor {
        build: "Code".into(),
        scheme: "vscode".into(),
        home: root.to_owned(),
        database: global.join("state.vscdb"),
        global,
        app: root.to_owned(),
        executable: root.join("Code.exe"),
        extensions,
    }
}

fn read_row(db: &Connection, key: &str) -> String {
    db.query_row("SELECT value FROM ItemTable WHERE key=?1", [key], |row| {
        row.get(0)
    })
    .unwrap()
}

#[test]
fn editor_uses_exact_windows_executable_name() {
    let root = tempfile::tempdir().unwrap();
    let install = root.path();
    fs::write(install.join("Updater.exe"), []).unwrap();
    let product = json!({"nameShort":"VS Code Insiders","applicationName":"code-insiders"});
    assert!(editor::windows_executable(install, "Code - Insiders", &product).is_none());
    let exe = install.join("Code - Insiders.exe");
    fs::write(&exe, []).unwrap();
    assert_eq!(
        editor::windows_executable(install, "Code - Insiders", &product),
        Some(exe)
    );
}

#[test]
fn installed_entries_resolve_locations_and_retain_index_metadata() {
    let root = tempfile::tempdir().unwrap();
    let editor = fixture(root.path());
    let relative = editor.extensions.join("one-1.0.0");
    let uri_path = editor.extensions.join("second # suffix");
    fs::create_dir(&relative).unwrap();
    fs::create_dir(&uri_path).unwrap();
    fs::write(
        relative.join("package.json"),
        r#"{"displayName":"%displayName%","version":"old"}"#,
    )
    .unwrap();
    fs::write(
        relative.join("package.nls.json"),
        r#"{"displayName":"Local name"}"#,
    )
    .unwrap();
    fs::write(uri_path.join("package.json"), r#"{"name":"URI package"}"#).unwrap();
    let uri = url::Url::from_file_path(&uri_path).unwrap();
    let entries = json!([
        {"identifier":{"id":"fixture.one"},"location":"one-1.0.0","version":"1.0.0"},
        {"identifier":{"id":"fixture.two"},"location":uri.as_str(),"version":"2.0.0"},
        {"identifier":{"id":"fixture.three"},"location":"missing","version":"3.0.0"}
    ]);
    fs::write(
        editor.extensions.join("extensions.json"),
        entries.to_string(),
    )
    .unwrap();
    let rows = editor.installed().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["name"], "Local name");
    assert_eq!(rows[0]["version"], "1.0.0");
    assert_eq!(rows[1]["name"], "URI package");
    assert_eq!(rows[2]["name"], "fixture.three");
    assert_eq!(rows[2]["version"], "3.0.0");
}

#[test]
fn removes_only_active_history_key_and_matching_entry_kind() {
    let root = tempfile::tempdir().unwrap();
    let editor = fixture(root.path());
    let db = Connection::open(&editor.database).unwrap();
    db.execute_batch("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
        .unwrap();
    let local = "file:///fixture/workspace";
    let active = json!({"entries":[{"folderUri":local},{"fileUri":local}],"retained":true});
    for (key, value) in [
        ("recently.opened", active.to_string()),
        ("history.recentlyOpenedPathsList", "not JSON".to_owned()),
    ] {
        db.execute("INSERT INTO ItemTable VALUES (?1, ?2)", [key, &value])
            .unwrap();
    }
    let backup = "file:///fixture/backup";
    fs::write(
        editor.global.join("storage.json"),
        json!({"backupWorkspaces":{"folders":[{"folderUri":backup}]}}).to_string(),
    )
    .unwrap();
    let (rows, database_keys) = editor::recent_with_keys(&editor).unwrap();
    assert_eq!(rows.len(), 3);
    assert!(database_keys.contains(&format!("folder:{local}")));
    assert!(database_keys.contains(&format!("file:{local}")));
    assert!(!database_keys.contains(&format!("folder:{backup}")));
    editor::remove_recent(&editor, Some(&format!("folder:{local}"))).unwrap();
    let active: Value = serde_json::from_str(&read_row(&db, "recently.opened")).unwrap();
    assert_eq!(
        active,
        json!({"entries":[{"fileUri":local}],"retained":true})
    );
    assert_eq!(read_row(&db, "history.recentlyOpenedPathsList"), "not JSON");
    editor::remove_recent(&editor, None).unwrap();
    assert_eq!(read_row(&db, "recently.opened"), r#"{"entries":[]}"#);
    assert_eq!(
        read_row(&db, "history.recentlyOpenedPathsList"),
        r#"{"entries":[]}"#
    );
    assert_eq!(
        editor::recent(&editor).unwrap(),
        vec![json!({"folderUri":backup})]
    );
}

#[test]
fn legacy_key_updates_without_creating_current_key() {
    let root = tempfile::tempdir().unwrap();
    let editor = fixture(root.path());
    let db = Connection::open(&editor.database).unwrap();
    db.execute_batch("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
        .unwrap();
    let uri = "vscode-remote://ssh-remote+fixture/opt/project";
    let row = json!({"entries":[{"folderUri":uri}]}).to_string();
    db.execute(
        "INSERT INTO ItemTable VALUES (?1, ?2)",
        ["history.recentlyOpenedPathsList", &row],
    )
    .unwrap();
    editor::remove_recent(&editor, Some(&format!("folder:{uri}"))).unwrap();
    assert_eq!(
        read_row(&db, "history.recentlyOpenedPathsList"),
        r#"{"entries":[]}"#
    );
    let count: u64 = db
        .query_row("SELECT count(*) FROM ItemTable", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
