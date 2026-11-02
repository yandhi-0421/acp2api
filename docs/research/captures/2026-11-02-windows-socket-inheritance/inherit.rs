// Temporary probe of #223: does a closed client socket stay open in a child process?
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::AsRawSocket;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[link(name = "kernel32")]
extern "system" {
    fn GetHandleInformation(handle: *mut core::ffi::c_void, flags: *mut u32) -> i32;
}
fn inherit(socket: &TcpStream) -> String {
    let mut flags = 0u32;
    let ok = unsafe { GetHandleInformation(socket.as_raw_socket() as _, &mut flags) };
    format!("ok={ok} inherit={}", flags & 1)
}
// Runs a variant: returns the delay from the last close to EOF at the server.
fn variant(name: &str, clone: bool, close_clone_first: bool, child: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let client = TcpStream::connect(address).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    server.set_read_timeout(Some(Duration::from_secs(12))).unwrap();
    let mut duplicate = clone.then(|| client.try_clone().unwrap());
    println!("{name}: client {}", inherit(&client));
    if let Some(duplicate) = &duplicate {
        println!("{name}: duplicate {}", inherit(duplicate));
    }
    if close_clone_first {
        duplicate = None;
    }
    let mut holder = child.then(|| {
        Command::new("cmd")
            .args(["/c", "ping -n 9 127.0.0.1 >NUL"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    });
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut byte = [0u8; 1];
        let result = server.read(&mut byte);
        sender.send((Instant::now(), format!("{result:?}"))).unwrap();
    });
    drop(duplicate);
    drop(client);
    let closed = Instant::now();
    let (end, result) = receiver.recv().unwrap();
    println!("{name}: EOF after {} ms, read {result}", end.duration_since(closed).as_millis());
    if let Some(holder) = &mut holder {
        let _ = holder.kill();
        let _ = holder.wait();
    }
}
fn listeners() {
    use std::os::windows::io::AsRawSocket;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let raw = |h: u64| {
        let mut flags = 0u32;
        let ok = unsafe { GetHandleInformation(h as _, &mut flags) };
        format!("ok={ok} inherit={}", flags & 1)
    };
    println!("listener {}", raw(listener.as_raw_socket()));
    let address = listener.local_addr().unwrap();
    let _a = TcpStream::connect(address).unwrap();
    let (accepted, _) = listener.accept().unwrap();
    println!("accepted from the first listener {}", inherit(&accepted));
    let duplicate = listener.try_clone().unwrap();
    println!("duplicate listener {}", raw(duplicate.as_raw_socket()));
    let _b = TcpStream::connect(address).unwrap();
    let (accepted, _) = duplicate.accept().unwrap();
    println!("accepted from the duplicate listener {}", inherit(&accepted));
}
fn main() {
    listeners();
    variant("no child, no clone", false, false, false);
    variant("no child, clone", true, false, false);
    variant("child, no clone", false, false, true);
    variant("child, clone open at the start", true, false, true);
    variant("child, clone closed before the start", true, true, true);
}
