use serde_json::{json, Value};
use tezbar_extension_sdk::{Extension, ExtensionPaths, TestContext};
use tezbar_visual_studio_code::{editor::Editor, Port};
fn fixture(root: &std::path::Path) -> Editor {
    let global = root.join("global");
    std::fs::create_dir_all(&global).unwrap();
    let extensions = root.join("extensions");
    std::fs::create_dir_all(&extensions).unwrap();
    Editor {
        build: "Code".into(),
        scheme: "vscode".into(),
        home: root.into(),
        app: root.join("app"),
        executable: root.join("fixture.exe"),
        database: global.join("state.vscdb"),
        global,
        extensions,
    }
}
fn test_context(root: &std::path::Path) -> TestContext {
    TestContext::new(
        ExtensionPaths {
            package_dir: root.into(),
            data_dir: root.join("data"),
            temp_dir: root.join("tmp"),
        },
        &[
            "clipboard.write",
            "shell",
            "selection",
            "window.visibility",
            "settings.open",
            "host.quicklinks.create",
        ],
    )
}
fn action(v: &Value, title: &str) -> String {
    v["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"] == title)
        .unwrap_or_else(|| panic!("missing {title}: {v}"))
        .get("id")
        .unwrap()
        .as_str()
        .unwrap()
        .into()
}
fn row_action(v: &Value, key: &str, title: &str) -> String {
    let row = v["root"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["props"]["id"] == key)
        .unwrap();
    v["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| {
            a["title"] == title
                && row["props"]["actionIds"]
                    .as_array()
                    .unwrap()
                    .contains(&a["id"])
        })
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}
fn database(editor: &Editor, entries: Value) -> rusqlite::Connection {
    let db = rusqlite::Connection::open(&editor.database).unwrap();
    db.execute_batch("CREATE TABLE ItemTable (key TEXT PRIMARY KEY,value TEXT)")
        .unwrap();
    db.execute(
        "INSERT INTO ItemTable VALUES ('recently.opened',?1)",
        [json!({"entries":entries,"preserved":true}).to_string()],
    )
    .unwrap();
    db
}
#[tokio::test]
async fn native_grid_copy_quicklink_and_stale_effect() {
    let root = tempfile::tempdir().unwrap();
    let editor = fixture(root.path());
    let folder = root.path().join("Folder #100% Unicode");
    std::fs::create_dir(&folder).unwrap();
    let uri = url::Url::from_directory_path(&folder).unwrap().to_string();
    let key = format!("folder:{uri}");
    let _db = database(&editor, json!([{"folderUri":uri}]));
    let t = test_context(root.path());
    let p = Port::with_editor(editor);
    let mut staged = json!({});
    let mut v = p
        .handle(
            &t.context,
            "index",
            "extension:run-command",
            &json!({"sessionId":"s","preferences":{"layout":"grid"}}),
            &mut staged,
        )
        .await
        .unwrap();
    assert_eq!(v["root"]["type"], "Grid");
    assert!(t.calls().is_empty());
    let copy = row_action(&v, &key, "Copy Name");
    t.reply(Ok(Value::Null));
    v = t
        .run(p.handle(
            &t.context,
            "index",
            "extension:invoke-action",
            &json!({"sessionId":"s","actionId":copy}),
            &mut staged,
        ))
        .await
        .unwrap();
    assert_eq!(t.calls()[0]["params"]["text"], "Folder #100% Unicode");
    p.handle(
        &t.context,
        "index",
        "extension:invoke-action",
        &json!({"sessionId":"s","actionId":copy}),
        &mut staged,
    )
    .await
    .unwrap();
    assert_eq!(t.calls().len(), 1);
    let link = row_action(&v, &key, "Create Quicklink");
    t.reply(Ok(Value::Null));
    t.run(p.handle(
        &t.context,
        "index",
        "extension:invoke-action",
        &json!({"sessionId":"s","actionId":link}),
        &mut staged,
    ))
    .await
    .unwrap();
    let calls = t.calls();
    let link = calls.last().unwrap()["params"]["link"].as_str().unwrap();
    let parsed = url::Url::parse(link).unwrap();
    assert!(parsed.fragment().is_none());
    assert!(link.contains("%23") && link.contains("%25"));
}
#[tokio::test]
async fn independent_views_merge_pins_and_confirm_clear_is_reachable() {
    let root = tempfile::tempdir().unwrap();
    let editor = fixture(root.path());
    let mut entries = vec![];
    let mut keys = vec![];
    for name in ["one", "two"] {
        let path = root.path().join(name);
        std::fs::create_dir(&path).unwrap();
        let uri = url::Url::from_directory_path(path).unwrap().to_string();
        keys.push(format!("folder:{uri}"));
        entries.push(json!({"folderUri":uri}));
    }
    let _db = database(&editor, json!(entries));
    let t = test_context(root.path());
    let p = Port::with_editor(editor);
    let mut staged = json!({});
    let mut views = vec![];
    for id in ["one", "two"] {
        views.push(
            p.handle(
                &t.context,
                "index",
                "extension:run-command",
                &json!({"sessionId":id}),
                &mut staged,
            )
            .await
            .unwrap(),
        );
    }
    for (i, id) in ["one", "two"].iter().enumerate() {
        let pin = row_action(&views[i], &keys[i], "Pin");
        p.handle(
            &t.context,
            "index",
            "extension:invoke-action",
            &json!({"sessionId":id,"actionId":pin}),
            &mut staged,
        )
        .await
        .unwrap();
    }
    let pins: Value =
        serde_json::from_slice(&std::fs::read(root.path().join("data/pinned-v1.json")).unwrap())
            .unwrap();
    assert_eq!(pins.as_array().unwrap().len(), 2);
    let mut v = p
        .handle(
            &t.context,
            "index",
            "extension:refresh-session",
            &json!({"sessionId":"one"}),
            &mut staged,
        )
        .await
        .unwrap();
    let remove = action(&v, "Remove All Recent Projects…");
    v = p
        .handle(
            &t.context,
            "index",
            "extension:invoke-action",
            &json!({"sessionId":"one","actionId":remove}),
            &mut staged,
        )
        .await
        .unwrap();
    let confirm = action(&v, "Confirm Remove All Recent Projects");
    for row in v["root"]["children"].as_array().unwrap() {
        if let Some(ids) = row["props"]["actionIds"].as_array() {
            assert!(ids.contains(&json!(confirm)));
        }
    }
}
#[tokio::test]
async fn backup_only_hide_does_not_touch_locked_database() {
    let root = tempfile::tempdir().unwrap();
    let editor = fixture(root.path());
    let primary = root.path().join("primary");
    let backup = root.path().join("backup");
    std::fs::create_dir(&primary).unwrap();
    std::fs::create_dir(&backup).unwrap();
    let a = url::Url::from_directory_path(primary).unwrap().to_string();
    let b = url::Url::from_directory_path(backup).unwrap().to_string();
    let key = format!("folder:{b}");
    let db = database(&editor, json!([{"folderUri":a}]));
    std::fs::write(
        editor.global.join("storage.json"),
        json!({"backupWorkspaces":{"folders":[{"folderUri":b}]}}).to_string(),
    )
    .unwrap();
    let t = test_context(root.path());
    let p = Port::with_editor(editor);
    let mut staged = json!({});
    let v = p
        .handle(
            &t.context,
            "index",
            "extension:run-command",
            &json!({"sessionId":"s"}),
            &mut staged,
        )
        .await
        .unwrap();
    db.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let remove = row_action(&v, &key, "Remove from Recent Projects");
    let v = p
        .handle(
            &t.context,
            "index",
            "extension:invoke-action",
            &json!({"sessionId":"s","actionId":remove}),
            &mut staged,
        )
        .await
        .unwrap();
    assert!(!v["root"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["props"]["id"] == key));
    db.execute_batch("ROLLBACK").unwrap();
}
