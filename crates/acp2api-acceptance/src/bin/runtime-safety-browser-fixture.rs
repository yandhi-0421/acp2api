#![allow(
    clippy::disallowed_methods,
    reason = "The independent browser fixture needs real-time process and request caps"
)]
use acp2api_acceptance::{Scenario, runtime_safety_check as safety};
use serde_json::json;
use std::{
    io::{BufRead, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() {
    let result = tokio::time::timeout(Duration::from_secs(180), run())
        .await
        .map_err(|_| "overall fixture time cap".to_owned())
        .and_then(|result| result);
    if let Err(error) = result {
        eprintln!("Browser fixture stopped: {error}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), String> {
    println!(
        "Caps: 180 s total; 16 setup requests; 64 panel requests; 3 s per setup request; 4 MiB per-log capture; guarded fake ACP only. Send stop on stdin to clean up. Credential values are never printed."
    );
    let started = Instant::now();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| "setup listener")?;
    let address = listener
        .local_addr()
        .map_err(|_| "setup address")?
        .to_string();
    println!("Setup http://{address}/");
    let (mut browser, password) =
        tokio::time::timeout(Duration::from_secs(120), setup(listener, &address))
            .await
            .map_err(|_| "setup time cap")??;
    let mut scenario = Scenario::default();
    scenario.plan(safety::isolated());
    scenario
        .fake()
        .next_prompt(safety::execution_plan("browser_builtin"));
    let answer = safety::send(&mut scenario, "browser notice fixture").await;
    safety::error(&answer, "browser_builtin");
    safety::event_log(&scenario, "built_in_tool_ran", "browser_builtin");
    let mut panel = safety::panel(&mut scenario).await;
    let result = panel
        .submit("owner", "/manage/password", json!({"password":password}))
        .await;
    if result.status != 303 {
        scenario.finish(false).await;
        return Err("temporary password setup failed".into());
    }
    let location = format!("http://{}/", panel.address);
    let response = format!(
        "HTTP/1.1 303 See Other\r\nLocation: {location}\r\nContent-Length: 0\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n"
    );
    browser
        .write_all(response.as_bytes())
        .await
        .map_err(|_| "setup response")?;
    drop(browser);

    println!("Panel {location}");
    println!(
        "The browser is unauthenticated. Use the temporary password in the normal management password form. Check the built-in tool notice, reload, dismiss it, reload again, then send stop on stdin. Leave Show key closed for screenshots."
    );
    let stop = Arc::new(AtomicBool::new(false));
    let input_stop = stop.clone();
    std::thread::spawn(move || {
        let mut command = String::new();
        let _ = std::io::stdin().lock().take(16).read_line(&mut command);
        if command.trim() == "stop" {
            input_stop.store(true, Ordering::SeqCst);
        }
    });
    let hub = scenario.hub.clone();
    let baseline = hub.counts().http_started;
    loop {
        if stop.load(Ordering::SeqCst)
            || started.elapsed() >= Duration::from_secs(180)
            || hub.counts().http_started.saturating_sub(baseline) >= 64
            || hub.failure().is_some()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panel.login("audit", &password).await;
    let page = panel.page("audit").await;
    let html = acp2api_acceptance::panel_check::text(&page);
    let dismissed =
        !html.contains(&safety::notice("browser_builtin")) && html.contains("id=\"local-api-key\"");
    scenario.finish(dismissed).await;
    if !dismissed {
        return Err("browser must dismiss the real notice before stop".into());
    }
    println!(
        "PASS: browser dismissal remained effective on authenticated reload. Browser fixture cleaned up."
    );
    Ok(())
}
async fn setup(
    listener: tokio::net::TcpListener,
    address: &str,
) -> Result<(tokio::net::TcpStream, String), String> {
    for _ in 0..16 {
        let (mut stream, _) = listener.accept().await.map_err(|_| "setup accept")?;
        let received = tokio::time::timeout(Duration::from_secs(3), read_request(&mut stream))
            .await
            .map_err(|_| "setup request time cap")??;
        let (head, body) = received;
        let host = head.lines().find_map(|line| {
            line.strip_prefix("Host: ")
                .or_else(|| line.strip_prefix("host: "))
        });
        let origin = head.lines().find_map(|line| {
            line.strip_prefix("Origin: ")
                .or_else(|| line.strip_prefix("origin: "))
        });
        if host != Some(address)
            || origin.is_some_and(|origin| origin != format!("http://{address}"))
        {
            reply(&mut stream, 400, "Use the local fixture setup address.").await?;
            continue;
        }
        if head.starts_with("GET / HTTP/") {
            reply(&mut stream,200,r#"<!doctype html><html lang="en"><meta charset="utf-8"><title>Isolated panel check</title><h1>Browser check setup</h1><p>Choose a temporary password for this isolated check. The next page uses the normal management password form.</p><form method="post" action="/"><label for="temporary-password">Temporary test password</label><input type="password" id="temporary-password" name="password" required maxlength="128" autocomplete="new-password"><button>Start the browser check</button></form></html>"#).await?;
        } else if head.starts_with("POST / HTTP/") {
            let password = body
                .strip_prefix("password=")
                .and_then(decode_form)
                .filter(|s| !s.is_empty() && s.len() <= 128);
            if let Some(password) = password {
                return Ok((stream, password));
            }
            reply(
                &mut stream,
                400,
                "Enter a temporary password of at most 128 bytes.",
            )
            .await?;
        } else {
            reply(&mut stream, 404, "Not found.").await?;
        }
    }
    Err("setup request cap".into())
}
async fn read_request(stream: &mut tokio::net::TcpStream) -> Result<(String, String), String> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|_| "setup request read")?;
        if n == 0 {
            return Err("setup request ended".into());
        }
        bytes.extend_from_slice(&chunk[..n]);
        if bytes.len() > 8192 {
            return Err("setup request size cap".into());
        }
        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let head = std::str::from_utf8(&bytes[..end]).map_err(|_| "setup headers")?;
            let length = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|s| s.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if length > 1024 {
                return Err("setup body cap".into());
            }
            if bytes.len() >= end + 4 + length {
                return Ok((
                    head.to_owned(),
                    String::from_utf8(bytes[end + 4..end + 4 + length].to_vec())
                        .map_err(|_| "setup form")?,
                ));
            }
        }
    }
}
fn decode_form(text: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut input = text.bytes();
    while let Some(byte) = input.next() {
        bytes.push(match byte {
            b'+' => b' ',
            b'%' => {
                let a = (input.next()? as char).to_digit(16)?;
                let b = (input.next()? as char).to_digit(16)?;
                (a * 16 + b) as u8
            }
            other => other,
        });
    }
    String::from_utf8(bytes).ok()
}
async fn reply(stream: &mut tokio::net::TcpStream, status: u16, body: &str) -> Result<(), String> {
    let response = format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|_| "setup response".into())
}
