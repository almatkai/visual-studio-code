use serde_json::json;
use std::fs;
use tezbar_extension_sdk::{Extension, ExtensionPaths, TestContext};
use tezbar_visual_studio_code::{agents, editor::Editor, Port};

#[test]
fn standalone_agents_app_is_distinct_from_antigravity_ide() {
    let root = tempfile::tempdir().unwrap();
    let programs = root.path().join("AppData/Local/Programs");
    fs::create_dir_all(programs.join("Antigravity IDE")).unwrap();
    fs::write(programs.join("Antigravity IDE/Antigravity IDE.exe"), []).unwrap();
    let roots = vec![programs.clone()];
    assert!(agents::find_application("Antigravity", true, root.path(), &roots, &[]).is_err());
    fs::create_dir_all(programs.join("antigravity")).unwrap();
    let agents_app = programs.join("antigravity/Antigravity.exe");
    fs::write(&agents_app, []).unwrap();
    assert_eq!(
        agents::find_application("Antigravity", true, root.path(), &roots, &[]).unwrap(),
        agents_app
    );
}

#[test]
fn mac_user_apps_and_windows_system_installs_need_no_editor_profile() {
    let root = tempfile::tempdir().unwrap();
    let system = root.path().join("Applications");
    let user = root.path().join("home/Applications");
    fs::create_dir_all(user.join("Cursor.app")).unwrap();
    fs::create_dir_all(user.join("Antigravity IDE.app")).unwrap();
    let mac_roots = vec![system.clone(), user.clone()];
    assert_eq!(
        agents::find_application("Cursor", false, root.path(), &[], &mac_roots).unwrap(),
        user.join("Cursor.app")
    );
    assert!(agents::find_application("Antigravity", false, root.path(), &[], &mac_roots).is_err());
    fs::create_dir_all(system.join("Antigravity.app")).unwrap();
    assert_eq!(
        agents::find_application("Antigravity", false, root.path(), &[], &mac_roots).unwrap(),
        system.join("Antigravity.app")
    );
    let programs = root.path().join("Program Files");
    fs::create_dir_all(programs.join("Windsurf")).unwrap();
    fs::write(programs.join("Windsurf/Windsurf.exe"), []).unwrap();
    assert_eq!(
        agents::find_application("Windsurf", true, root.path(), &[programs.clone()], &[]).unwrap(),
        programs.join("Windsurf/Windsurf.exe")
    );
    assert!(agents::find_application("Unrecognized", true, root.path(), &[programs], &[]).is_err());
}

#[test]
fn registered_apps_support_custom_locations_and_shared_bundle_ids() {
    let apps = json!([
        {"name":"Devin", "bundleId":"com.exafunction.windsurf", "path":"/custom/Devin.app"},
        {"name":"Windsurf", "bundleId":"com.exafunction.windsurf", "path":"/other/Windsurf.app"},
        {"name":"Windsurf", "path":"D:\\custom install\\Windsurf.exe"},
        {"name":"Kiro", "path":"C:/Start Menu/Programs/Kiro.lnk"}
    ]);
    assert_eq!(
        agents::registered_application("Windsurf", false, &apps).unwrap(),
        "/other/Windsurf.app"
    );
    assert_eq!(
        agents::registered_application("Devin", false, &apps).unwrap(),
        "/custom/Devin.app"
    );
    assert_eq!(
        agents::registered_application("Kiro", true, &apps).unwrap(),
        "C:/Start Menu/Programs/Kiro.lnk"
    );
    assert!(agents::registered_application("VSCodium", false, &apps).is_none());
    assert!(agents::registered_application(
        "Kiro",
        true,
        &json!([{"name":"Kiro","path":"bad\u{0}path"}])
    )
    .is_none());
}

#[tokio::test]
async fn failed_editor_launch_keeps_launcher_visible() {
    let root = tempfile::tempdir().unwrap();
    let editor = Editor {
        build: "Code".into(),
        scheme: "vscode".into(),
        home: root.path().into(),
        global: root.path().join("global"),
        app: root.path().join("app"),
        executable: root.path().join("missing-editor.exe"),
        database: root.path().join("missing.db"),
        extensions: root.path().join("extensions"),
    };
    let context = TestContext::new(
        ExtensionPaths {
            package_dir: root.path().into(),
            data_dir: root.path().join("data"),
            temp_dir: root.path().join("tmp"),
        },
        &["shell", "window.visibility"],
    );
    let port = Port::with_editor(editor);
    let result = port
        .handle(
            &context.context,
            "open-agents-window",
            "extension:run-command",
            &json!({"sessionId":"failed","preferences":{"build":"Code"}}),
            &mut json!({}),
        )
        .await;
    assert!(result.is_err());
    assert!(context.calls().is_empty());
}
