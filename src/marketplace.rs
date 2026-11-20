use serde_json::{json, Value};
pub const ENDPOINT:&str="https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery?api-version=3.0-preview.1";
pub fn query(text: &str) -> Result<Value, String> {
    if text.len() > 2048 {
        return Err("Search text exceeds 2048 bytes".into());
    }
    Ok(
        json!({"filters":[{"criteria":[{"filterType":8,"value":"Microsoft.VisualStudio.Code"},{"filterType":10,"value":text},{"filterType":12,"value":"4096"}],"pageNumber":1,"pageSize":100,"sortBy":0,"sortOrder":0}],"assetTypes":[],"flags":438}),
    )
}
pub async fn search(
    client: &reqwest::Client,
    endpoint: &str,
    text: &str,
) -> Result<Vec<Value>, String> {
    if text.is_empty() {
        return Ok(vec![]);
    }
    let mut response = client
        .post(endpoint)
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&query(text)?).map_err(|_| "Invalid search")?)
        .send()
        .await
        .map_err(|_| "Marketplace request failed")?;
    if !response.status().is_success() {
        return Err(format!(
            "Marketplace returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let mut bytes = vec![];
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Marketplace response failed")?
    {
        if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
            return Err("Marketplace response exceeds 8 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let v: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Marketplace returned invalid JSON")?;
    let rows = v["results"][0]["extensions"]
        .as_array()
        .ok_or("Marketplace extensions missing")?;
    if rows.len() > 100 {
        return Err("Marketplace returned too many extensions".into());
    }
    let mut out = vec![];
    for e in rows {
        let id = format!(
            "{}.{}",
            e["publisher"]["publisherName"]
                .as_str()
                .ok_or("Invalid publisher")?,
            e["extensionName"]
                .as_str()
                .ok_or("Invalid extension name")?
        );
        if !crate::editor::extension_id(&id) {
            return Err("Invalid marketplace extension identity".into());
        }
        out.push(json!({"id":id,"name":e["displayName"].as_str().unwrap_or(&id),"description":e["shortDescription"],"version":e["versions"][0]["version"]}));
    }
    Ok(out)
}
