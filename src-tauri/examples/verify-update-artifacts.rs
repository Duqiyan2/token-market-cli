//! Build-host release verifier. Uses the pinned official updater and never installs.
//! JSON on stdin: { version, public_key, objects: [{platform,path,signature,role}] }.
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::Duration,
};
use tauri_plugin_updater::UpdaterExt;

#[tokio::main]
async fn main() {
    match verify().await {
        Ok(result) => println!("{result}"),
        Err(_) => {
            // Do not echo inputs, child output or environment into release logs.
            eprintln!("Updater artifact signature/version verification failed");
            std::process::exit(1);
        }
    }
}

async fn verify() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let input: serde_json::Value = serde_json::from_str(&input)?;
    if input["mode"] == "manifest" {
        return verify_manifest(&input).await;
    }
    let version = input["version"].as_str().ok_or("version required")?;
    let key = input["public_key"].as_str().ok_or("public key required")?;
    let objects = input["objects"].as_array().ok_or("objects required")?;
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.package_info_mut().version = "0.0.0".parse()?;
    context.config_mut().plugins.0.insert(
        "updater".into(),
        serde_json::json!({
            "pubkey":"", "requireSignedVersion":true, "dangerousInsecureTransportProtocol":true
        }),
    );
    let app = tauri::test::mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)?;
    let executable = tempfile::tempdir()?;
    let mut verified = 0;
    for object in objects.iter().filter(|item| item["role"] == "updater") {
        let target = object["platform"].as_str().ok_or("target required")?;
        let signature = object["signature"].as_str().ok_or("signature required")?;
        // Only metadata crosses this private loopback socket. Bytes are verified
        // directly through Update::verify; no OS installation method is called.
        let body = serde_json::json!({"version":version,"platforms":{
            (target):{"url":"https://invalid.invalid/unused","signature":signature}
        }})
        .to_string();
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let endpoint = format!("http://{}/fixture.json", listener.local_addr()?);
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
        });
        let update = app
            .updater_builder()
            .endpoints(vec![endpoint.parse()?])?
            .pubkey(key)
            .target(target)
            .no_proxy()
            .executable_path(
                executable
                    .path()
                    .join("Token Market.app/Contents/MacOS/Token Market"),
            )
            .timeout(Duration::from_secs(10))
            .build()?
            .check()
            .await?;
        worker.join().map_err(|_| "metadata server failed")?;
        let update = update.ok_or("update missing")?;
        let bytes = std::fs::read(object["path"].as_str().ok_or("path required")?)?;
        update.verify(&bytes)?;
        verified += 1;
    }
    if verified != input["required_targets"].as_u64().unwrap_or(2) {
        return Err("two targets required".into());
    }
    Ok(serde_json::json!({"verified":true,"verifier":"tauri-plugin-updater/2.14.1"}))
}

// This checks the actual static manifest schema for both targets. It never
// downloads an installer or invokes any OS installation/restart operation.
async fn verify_manifest(
    input: &serde_json::Value,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let manifest = &input["manifest"];
    let latest = manifest["version"].as_str().ok_or("version required")?;
    let key = input["public_key"].as_str().ok_or("public key required")?;
    let bytes = serde_json::to_vec(manifest)?;
    let mut checks = Vec::new();
    for current in [latest, "0.0.0"] {
        for target in ["darwin-aarch64", "windows-x86_64-nsis"] {
            let mut context = tauri::test::mock_context(tauri::test::noop_assets());
            context.package_info_mut().version = current.parse()?;
            context.config_mut().plugins.0.insert("updater".into(), serde_json::json!({
                "pubkey":"", "requireSignedVersion":true, "dangerousInsecureTransportProtocol":true
            }));
            let app = tauri::test::mock_builder()
                .plugin(tauri_plugin_updater::Builder::new().build())
                .build(context)?;
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let endpoint = format!("http://{}/stable.json", listener.local_addr()?);
            let body = bytes.clone();
            let worker = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut request = [0; 4096];
                let _ = stream.read(&mut request);
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(&body);
            });
            let executable = tempfile::tempdir()?;
            let result = app
                .updater_builder()
                .endpoints(vec![endpoint.parse()?])?
                .pubkey(key)
                .target(target)
                .no_proxy()
                .executable_path(
                    executable
                        .path()
                        .join("Token Market.app/Contents/MacOS/app"),
                )
                .timeout(Duration::from_secs(10))
                .build()?
                .check()
                .await;
            worker.join().map_err(|_| "metadata server failed")?;
            let update = result?;
            if current == latest {
                if update.is_some() {
                    return Err("same version must be current".into());
                }
                checks.push(serde_json::json!({"target":target,"current_version":current,"status":"current"}));
            } else {
                let update = update.ok_or("newer version missing")?;
                if update.version != latest
                    || update.download_url.as_str()
                        != manifest["platforms"][target]["url"]
                            .as_str()
                            .ok_or("URL required")?
                {
                    return Err("discovered version/URL differs".into());
                }
                checks.push(serde_json::json!({"target":target,"current_version":current,"status":"available","version":latest}));
            }
        }
    }
    Ok(
        serde_json::json!({"verified":true,"verifier":"tauri-plugin-updater/2.14.1","checks":checks}),
    )
}
