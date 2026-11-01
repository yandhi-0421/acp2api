use std::{
    future::{Future, poll_fn},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    task::Poll,
    time::Duration,
};

#[tokio::main]
async fn main() {
    let mode = std::env::args().nth(1).expect("one control mode");
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        let address: SocketAddr = "192.0.2.1:9".parse().unwrap();
        match mode.as_str() {
            "short" => {
                let mut connect =
                    Box::pin(tokio::net::TcpSocket::new_v4().unwrap().connect(address));
                poll_fn(|cx| {
                    let _ = connect.as_mut().poll(cx);
                    Poll::Ready(())
                })
                .await;
                drop(connect);
            }
            "failed" => {
                assert!(
                    TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_err(),
                    "TEST-NET control must fail"
                );
            }
            "loopback" => {
                let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
                let client = TcpStream::connect_timeout(
                    &listener.local_addr().unwrap(),
                    Duration::from_millis(200),
                )
                .unwrap();
                drop(client);
            }
            "ipv6" => {
                let address = "[::1]:9".parse().unwrap();
                let _ = TcpStream::connect_timeout(&address, Duration::from_millis(200));
            }
            #[cfg(target_os = "macos")]
            "other-loopback" => {
                let address = "127.0.0.2:9".parse().unwrap();
                let _ = TcpStream::connect_timeout(&address, Duration::from_millis(200));
            }
            _ => panic!("unknown control mode"),
        }
    })
    .await;
    result.expect("one connection attempt; two-second control cap");
    println!("{}", serde_json::json!({"mode":mode}));
}
