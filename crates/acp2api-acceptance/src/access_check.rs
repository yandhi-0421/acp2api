use crate::real_limit;
use crate::{Scenario, client::Answer, endpoint_check, journal_check};
use std::net::{IpAddr, Ipv4Addr};

pub const KEY_MESSAGE: &str = "acp2api: the local API key is absent or wrong.";
#[derive(Clone, Copy, Debug)]
pub enum Credential {
    Absent,
    Wrong,
    Key,
    Bearer,
    WrongBearer,
}
pub async fn send(
    s: &mut Scenario,
    endpoint: &str,
    credential: Credential,
    headers: Vec<(String, String)>,
    stream: bool,
) -> Answer {
    let mut call = endpoint_check::request(endpoint, "Access check", stream);
    s.start().await;
    let client = s.client().await.clone();
    let client = match credential {
        Credential::Key => client,
        _ => client.without_local_api_key(),
    };
    match credential {
        Credential::Wrong => call
            .headers
            .push(("x-api-key".into(), crate::setup::random_hex(24))),
        Credential::Bearer => call
            .headers
            .push(("authorization".into(), format!("Bearer {}", s.root.key))),
        Credential::WrongBearer => call.headers.push((
            "authorization".into(),
            format!("Bearer {}", crate::setup::random_hex(24)),
        )),
        _ => {}
    }
    call.headers.extend(headers);
    client.send(call).finish(&mut s.time).await
}
pub fn no_cors(answer: &Answer) {
    assert!(
        answer
            .headers
            .keys()
            .all(|h| !h.to_ascii_lowercase().starts_with("access-control-")),
        "FR-008 no Access-Control-* response header"
    );
}
pub fn key_refusal(answer: &Answer, endpoint: &str) {
    assert_eq!(
        answer.status, 401,
        "FR-007 key refusal before HTTP 200 or SSE"
    );
    refusal(answer, endpoint);
    assert_eq!(answer.json()["error"]["message"], KEY_MESSAGE);
    let body = answer.json();
    if endpoint == "POST /v1/messages" {
        assert_eq!(body["error"]["type"], "authentication_error");
    } else {
        assert_eq!(body["error"]["code"], "invalid_api_key");
        assert_eq!(body["error"]["type"], "invalid_request_error");
        assert!(body["error"]["param"].is_null());
    }
}
pub fn refusal(answer: &Answer, endpoint: &str) {
    assert!(
        (400..500).contains(&answer.status),
        "refusal status: {}",
        answer.status
    );
    assert!(answer.events.is_empty(), "no SSE before refusal");
    assert!(
        answer.headers["content-type"].starts_with("application/json"),
        "JSON refusal, including streamed input"
    );
    let body = answer.json();
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|v| !v.is_empty()),
        "error envelope has a message"
    );
    if endpoint == "POST /v1/messages" {
        assert_eq!(body["type"], "error");
    }
    no_cors(answer);
}
pub fn other_port(port: u16) -> u16 {
    if port == 65535 { 65534 } else { port + 1 }
}
pub fn prompt_count(s: &Scenario) -> usize {
    s.fake().record().prompts().len()
}
pub async fn journal(s: &Scenario, answer: &Answer, expected_total: usize) {
    journal_check::wait_terminal(s, expected_total).await;
    let records = journal_check::records(s);
    let terminal = journal_check::terminal(s);
    assert_eq!(
        records.len(),
        expected_total,
        "one typed record per API request"
    );
    assert_eq!(
        terminal.len(),
        expected_total,
        "one terminal line per API request"
    );
    let id = answer
        .headers
        .get("x-request-id")
        .expect("request identifier on guard response");
    let rows: Vec<_> = records
        .iter()
        .filter(|r| r["request_id"].as_str() == Some(id))
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        terminal
            .iter()
            .filter(|r| r["request_id"].as_str() == Some(id))
            .count(),
        1
    );
    assert_eq!(
        rows[0]["result"],
        if answer.status == 200 {
            "success"
        } else {
            "refused"
        }
    );
    if answer.status == 401 {
        assert_eq!(rows[0]["error_code"], "invalid_api_key");
    }
}
/// Enumerate assigned interface addresses without sending a discovery packet.
#[cfg(unix)]
pub fn network_address() -> Option<IpAddr> {
    let mut first = std::ptr::null_mut();
    // getifaddrs owns a linked list until freeifaddrs below.
    if unsafe { libc::getifaddrs(&mut first) } != 0 {
        return None;
    }
    let mut item = first;
    let mut address = None;
    while !item.is_null() {
        let interface = unsafe { &*item };
        if !interface.ifa_addr.is_null()
            && unsafe { (*interface.ifa_addr).sa_family as i32 } == libc::AF_INET
        {
            let socket = unsafe { &*(interface.ifa_addr as *const libc::sockaddr_in) };
            let ip = Ipv4Addr::from(socket.sin_addr.s_addr.to_ne_bytes());
            if !ip.is_loopback() && !ip.is_unspecified() {
                address = Some(IpAddr::V4(ip));
                break;
            }
        }
        item = interface.ifa_next;
    }
    unsafe {
        libc::freeifaddrs(first);
    }
    address
}
#[cfg(not(unix))]
pub fn network_address() -> Option<IpAddr> {
    // Bind and connect UDP locally to obtain the OS-selected assigned address.
    let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    let local_name = std::env::var("COMPUTERNAME").ok()?;
    use std::net::ToSocketAddrs;
    let address = (local_name.as_str(), 0)
        .to_socket_addrs()
        .ok()?
        .find(|a| a.is_ipv4() && !a.ip().is_loopback())?;
    socket.connect(address).ok()?;
    Some(socket.local_addr().ok()?.ip())
}
pub async fn connect(address: IpAddr, port: u16) -> bool {
    // A loopback connection must succeed; another address is the window of "accepts no connection".
    let limit = if address.is_loopback() {
        real_limit(std::time::Duration::from_secs(2))
    } else {
        std::time::Duration::from_secs(2)
    };
    matches!(
        tokio::time::timeout(limit, tokio::net::TcpStream::connect((address, port))).await,
        Ok(Ok(_))
    )
}

#[cfg(test)]
mod tests {
    use crate::real_limit;
    use crate::{
        client::{ApiCall, ApiClient},
        hub::Hub,
        timeline::TimeLine,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[tokio::test]
    async fn credential_header_omission_is_verified_by_a_real_receiver() {
        for (key_present, bearer_present) in [(true, false), (false, false), (false, true)] {
            let hub = Hub::new(crate::setup::random_hex(24));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let observed_hub = hub.clone();
            let receiver = tokio::spawn(async move {
                tokio::time::timeout(real_limit(std::time::Duration::from_secs(3)),async move {
                    let (mut socket,_)=listener.accept().await.unwrap();
                    let mut bytes=Vec::new();
                    while !bytes.ends_with(b"\r\n\r\n") {
                        assert!(bytes.len()<8192,"receiver request cap: 8192 bytes");
                        bytes.push(socket.read_u8().await.unwrap());
                    }
                    let head=String::from_utf8(bytes).unwrap().to_ascii_lowercase();
                    let presence=(head.lines().any(|l|l.starts_with("x-api-key:")),head.lines().any(|l|l.starts_with("authorization:")));
                    observed_hub.shared.0.lock().unwrap().counts.http_started+=1;
                    observed_hub.shared.1.notify_all();
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
                    presence
                }).await.expect("receiver caps: 1 request, 3 seconds, no log")
            });
            let key = crate::setup::random_hex(24);
            let client = ApiClient::new(address, key.clone(), hub.clone());
            let client = if key_present {
                client
            } else {
                client.without_local_api_key()
            };
            let mut request = ApiCall::one_user_message("GET /v1/models", "");
            if bearer_present {
                request
                    .headers
                    .push(("authorization".into(), format!("Bearer {key}")));
            }
            let answer = client
                .send(request)
                .finish(&mut TimeLine::new(hub.clone()))
                .await;
            assert_eq!(answer.status, 200);
            assert_eq!(
                receiver.await.unwrap(),
                (key_present, bearer_present),
                "actual credential-header presence, values withheld"
            );
            hub.close();
        }
    }
}
