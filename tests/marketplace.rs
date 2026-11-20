use serde_json::{json,Value};
use tokio::io::{AsyncReadExt,AsyncWriteExt};
#[tokio::test]
async fn original_request_shape_and_owned_response(){
    let server=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let endpoint=format!("http://{}/gallery",server.local_addr().unwrap());
    let task=tokio::spawn(async move{
        let (mut socket,_)=server.accept().await.unwrap();let mut bytes=vec![];let (head,length)=loop{let mut chunk=[0u8;4096];let n=socket.read(&mut chunk).await.unwrap();assert!(n>0);bytes.extend_from_slice(&chunk[..n]);assert!(bytes.len()<8192);if let Some(end)=bytes.windows(4).position(|b|b==b"\r\n\r\n"){let header=String::from_utf8(bytes[..end].to_vec()).unwrap();let length=header.lines().find_map(|l|l.to_lowercase().strip_prefix("content-length:").and_then(|s|s.trim().parse::<usize>().ok())).unwrap();break(end+4,length);}};
        while bytes.len()<head+length{let mut chunk=[0u8;4096];let n=socket.read(&mut chunk).await.unwrap();assert!(n>0);bytes.extend_from_slice(&chunk[..n]);}
        let request:Value=serde_json::from_slice(&bytes[head..head+length]).unwrap();assert_eq!(request["flags"],438);assert_eq!(request["filters"][0]["criteria"][1]["value"],"owned query");assert_eq!(request["filters"][0]["pageSize"],100);
        let body=json!({"results":[{"extensions":[{"publisher":{"publisherName":"owned"},"extensionName":"fixture","displayName":"Owned Fixture","shortDescription":"Synthetic","versions":[{"version":"1.0.0"}]}]}]}).to_string();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    });
    let client=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(5)).build().unwrap();
    let rows=tezbar_visual_studio_code::marketplace::search(&client,&endpoint,"owned query").await.unwrap();assert_eq!(rows[0]["id"],"owned.fixture");task.await.unwrap();
    assert!(tezbar_visual_studio_code::marketplace::query(&"x".repeat(2049)).is_err());
}
