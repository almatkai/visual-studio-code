//! Owned editor process fixture. Never included in the distributable package.
use serde_json::{json,Value};
use std::path::{Path,PathBuf};
fn install(root:&Path,id:&str){
    let path=root.join("extensions").join(format!("{id}-1.0.0"));std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("package.json"),json!({"name":id,"displayName":id,"version":"1.0.0","description":"Owned fixture"}).to_string()).unwrap();
    let index=root.join("extensions/extensions.json");let mut rows:Vec<Value>=std::fs::read(&index).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or_default();rows.retain(|r|r["identifier"]["id"]!=id);rows.push(json!({"identifier":{"id":id},"version":"1.0.0","location":format!("{id}-1.0.0")}));std::fs::write(index,serde_json::to_vec(&rows).unwrap()).unwrap();
}
#[tokio::main(worker_threads=2)]
async fn main(){
    let args:Vec<_>=std::env::args().skip(1).collect();
    if args.first().is_some_and(|s|s.ends_with("cli.js")) {
        let root=Path::new(&args[0]).ancestors().find(|p|p.join("fixture-root.marker").is_file()).expect("owned fixture root");
        std::fs::write(root.join("cli-last.json"),serde_json::to_vec(&args[1..]).unwrap()).unwrap();
        if let Some(i)=args.iter().position(|a|a=="--install-extension"){install(root,&args[i+1]);}
        if let Some(i)=args.iter().position(|a|a=="--uninstall-extension"){let path=root.join("extensions/extensions.json");let mut rows:Vec<Value>=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();rows.retain(|r|r["identifier"]["id"]!=args[i+1]);std::fs::write(path,serde_json::to_vec(&rows).unwrap()).unwrap();}
        return;
    }
    let root=PathBuf::from(args.first().expect("owned fixture root"));
    std::fs::write(root.join("fixture-root.marker"), b"owned").unwrap();
    for dir in ["global","extensions","Fixture Editor.app/Contents/Resources/app/out","Project #100%",".git"]{std::fs::create_dir_all(root.join(dir)).unwrap();}
    std::fs::write(root.join(".git/HEAD"),"ref: refs/heads/fixture-owner\n").unwrap();
    let local=url::Url::from_directory_path(root.join("Project #100%")).unwrap().to_string();
    let db=rusqlite::Connection::open(root.join("global/state.vscdb")).unwrap();db.execute_batch("CREATE TABLE IF NOT EXISTS ItemTable (key TEXT PRIMARY KEY,value TEXT)").unwrap();
    db.execute("INSERT OR REPLACE INTO ItemTable VALUES ('recently.opened',?1)",[json!({"entries":[{"folderUri":local},{"folderUri":"vscode-remote://ssh-remote+owned/tmp/project","remoteAuthority":"ssh-remote+owned"},{"workspace":{"configPath":"vscode-remote://ssh-remote+owned/tmp/custom"},"remoteAuthority":"ssh-remote+owned"}]}).to_string()]).unwrap();drop(db);
    install(&root,"fixture.test");
    let editor=tezbar_visual_studio_code::editor::Editor{build:"Code".into(),scheme:"vscode".into(),home:root.clone(),app:root.join("Fixture Editor.app/Contents/Resources/app"),executable:std::env::current_exe().unwrap(),database:root.join("global/state.vscdb"),global:root.join("global"),extensions:root.join("extensions")};
    let dir=editor.global.join("tonka3000.raycast/transit");std::fs::create_dir_all(&dir).unwrap();
    let producer=tokio::spawn(async move{
        let mut last=String::new();loop{
            if let Ok(bytes)=std::fs::read(dir.join("request.json")){if let Ok(v)=serde_json::from_slice::<Value>(&bytes){if let Some(response)=v["args"]["filename"].as_str(){
                let path=Path::new(response);if response!=last&&path.parent()==Some(dir.as_path()){std::fs::write(path,json!([{"command":"fixture.run#100%","title":"Fixture Run","category":"Owned"}]).to_string()).unwrap();last=response.into();}
            }}}
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    });
    let result=tezbar_extension_sdk::serve(tezbar_visual_studio_code::Port::with_editor(editor)).await;producer.abort();result.unwrap();
}
