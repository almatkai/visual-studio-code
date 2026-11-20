use std::{fs, path::Path};
use tezbar_visual_studio_code::editor::resolve_installation;

fn product(app: &Path) {
    fs::create_dir_all(app).unwrap();
    fs::write(app.join("product.json"), r#"{"nameShort":"Code"}"#).unwrap();
    fs::create_dir_all(app.join("out")).unwrap();
    fs::write(app.join("out/cli.js"), "owned test CLI").unwrap();
}

#[test]
fn stale_user_directory_does_not_hide_complete_windows_system_install() {
    let root = tempfile::tempdir().unwrap();
    let stale = root.path().join("User/Programs/Microsoft VS Code");
    let valid = root.path().join("Program Files/Microsoft VS Code");
    fs::create_dir_all(&stale).unwrap();
    product(&valid.join("resources/app"));
    fs::write(valid.join("Code.exe"), []).unwrap();
    assert_eq!(
        resolve_installation("Code", true, &[stale.clone(), valid.clone()])
            .unwrap()
            .1,
        valid.join("Code.exe")
    );
    product(&stale.join("resources/app"));
    fs::write(stale.join("Updater.exe"), []).unwrap();
    assert_eq!(
        resolve_installation("Code", true, &[stale.clone(), valid.clone()])
            .unwrap()
            .1,
        valid.join("Code.exe")
    );
    fs::write(stale.join("Code.exe"), []).unwrap();
    fs::remove_file(stale.join("resources/app/out/cli.js")).unwrap();
    assert_eq!(
        resolve_installation("Code", true, &[stale.clone(), valid.clone()])
            .unwrap()
            .1,
        valid.join("Code.exe")
    );
    fs::remove_file(valid.join("Code.exe")).unwrap();
    assert!(resolve_installation("Code", true, &[stale, valid]).is_err());
}

#[test]
fn incomplete_system_bundle_does_not_hide_complete_mac_user_bundle() {
    let root = tempfile::tempdir().unwrap();
    let stale = root.path().join("Applications/Visual Studio Code.app");
    let valid = root.path().join("home/Applications/Visual Studio Code.app");
    product(&stale.join("Contents/Resources/app"));
    product(&valid.join("Contents/Resources/app"));
    fs::create_dir_all(valid.join("Contents/MacOS")).unwrap();
    fs::write(valid.join("Contents/MacOS/Electron"), []).unwrap();
    assert_eq!(
        resolve_installation("Code", false, &[stale.clone(), valid.clone()])
            .unwrap()
            .1,
        valid.join("Contents/MacOS/Electron")
    );
    fs::write(
        stale.join("Contents/Resources/app/product.json"),
        "not JSON",
    )
    .unwrap();
    assert_eq!(
        resolve_installation("Code", false, &[stale, valid.clone()])
            .unwrap()
            .0,
        valid.join("Contents/Resources/app")
    );
}

#[test]
fn stale_windows_resources_do_not_hide_complete_versioned_resources() {
    let root = tempfile::tempdir().unwrap();
    let install = root.path().join("Microsoft VS Code");
    let stale = install.join("resources/app");
    let complete = install.join("0123456789/resources/app");
    product(&stale);
    fs::remove_file(stale.join("out/cli.js")).unwrap();
    product(&complete);
    fs::write(install.join("Code.exe"), []).unwrap();
    assert_eq!(
        resolve_installation("Code", true, &[install.clone()])
            .unwrap()
            .0,
        complete
    );
    fs::write(stale.join("product.json"), "not JSON").unwrap();
    assert_eq!(
        resolve_installation("Code", true, &[install.clone()])
            .unwrap()
            .0,
        complete
    );
    fs::remove_file(stale.join("product.json")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let broken_version = install.join("fedcba9876/resources/app");
    product(&broken_version);
    fs::write(broken_version.join("product.json"), "invalid newer product").unwrap();
    assert_eq!(
        resolve_installation("Code", true, &[install]).unwrap().0,
        complete
    );
}
