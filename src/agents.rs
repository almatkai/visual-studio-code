//! Application launch discovery for the upstream Open Agents Window command.
//! It deliberately does not require an editor profile or product.json.
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn uses_cli(build: &str) -> bool {
    matches!(build, "Code" | "Code - Insiders")
}

pub fn registered_application(build: &str, windows: bool, apps: &Value) -> Option<String> {
    let bundle = match build {
        "IBM Bob" => "com.ibm.software.bob",
        "Kiro" => "dev.kiro.desktop",
        "Positron" => "com.rstudio.positron",
        "Qoder" => "com.qoder.ide",
        "Trae" => "com.trae.app",
        "Trae CN" => "cn.trae.app",
        "VSCodium" => "com.vscodium",
        "VSCodium - Insiders" => "com.vscodium.VSCodiumInsiders",
        "Devin" | "Windsurf" => "com.exafunction.windsurf",
        "Lingma" => "com.aliyun.lingma.ide",
        _ => return None,
    };
    apps.as_array()?.iter().find_map(|app| {
        let path = app["path"]
            .as_str()
            .filter(|p| !p.trim().is_empty() && !p.contains('\0'))?;
        let matches = if windows {
            let name = if build == "Trae CN" {
                "Trae - CN"
            } else {
                build
            };
            let executable = format!("{name}.exe");
            app["name"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(build))
                || path
                    .rsplit(['/', '\\'])
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&executable))
        } else {
            app["bundleId"] == bundle
                && (!matches!(build, "Devin" | "Windsurf")
                    || path
                        .to_lowercase()
                        .contains(&format!("{}.app", build.to_lowercase())))
        };
        matches.then(|| path.to_owned())
    })
}

pub fn find_application(
    build: &str,
    windows: bool,
    home: &Path,
    windows_roots: &[PathBuf],
    mac_roots: &[PathBuf],
) -> Result<PathBuf, String> {
    let definition = crate::editor::BUILDS
        .iter()
        .find(|entry| entry.0 == build)
        .ok_or("Unknown editor build")?;
    let candidates: Vec<_> = if windows {
        if matches!(build, "Antigravity" | "Cursor") {
            let (folder, executable) = if build == "Antigravity" {
                ("antigravity", "Antigravity.exe")
            } else {
                ("cursor", "Cursor.exe")
            };
            vec![home
                .join("AppData/Local/Programs")
                .join(folder)
                .join(executable)]
        } else {
            let name = if build == "Trae CN" {
                "Trae - CN"
            } else {
                build
            };
            windows_roots
                .iter()
                .map(|root| root.join(definition.2).join(format!("{name}.exe")))
                .collect()
        }
    } else {
        mac_roots
            .iter()
            .map(|root| root.join(definition.3))
            .collect()
    };
    candidates
        .into_iter()
        .find(|path| {
            if windows {
                path.is_file()
            } else {
                path.is_dir()
            }
        })
        .ok_or_else(|| format!("{build} app not found. Is it installed?"))
}

pub fn production_application(build: &str) -> Result<PathBuf, String> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .ok_or("Home directory unavailable")?;
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData/Local"));
    let mut windows_roots = vec![local.join("Programs")];
    windows_roots.extend(
        ["ProgramFiles", "ProgramFiles(x86)"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from),
    );
    find_application(
        build,
        cfg!(windows),
        &home,
        &windows_roots,
        &[PathBuf::from("/Applications"), home.join("Applications")],
    )
}
