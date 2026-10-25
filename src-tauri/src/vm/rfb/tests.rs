use super::*;
use std::{
    net::TcpListener,
    sync::mpsc,
    thread,
};

/// What the fake server saw the client send after the handshake.
#[derive(Debug, PartialEq)]
enum Seen {
    Pointer(u8, u16, u16),
    Key(u32, bool),
}

/// A minimal RFB 3.8 server: one 4×2 screen, optional VNC auth, a resize on the second
/// capture, and a bell before each update.
fn serve(password: Option<&'static str>) -> (SocketAddr, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let take = |s: &mut TcpStream, n: usize| { let mut b = vec![0; n]; s.read_exact(&mut b).unwrap(); b };
        s.write_all(b"RFB 003.008\n").unwrap();
        assert_eq!(take(&mut s, 12), b"RFB 003.008\n");
        match password {
            None => { s.write_all(&[1, 1]).unwrap(); assert_eq!(take(&mut s, 1), [1]); }
            Some(password) => {
                s.write_all(&[1, 2]).unwrap();
                assert_eq!(take(&mut s, 1), [2]);
                let challenge = *b"0123456789abcdef";
                s.write_all(&challenge).unwrap();
                let ok = take(&mut s, 16) == crate::vm::vnc_auth::response(password, &challenge);
                s.write_all(&u32::from(!ok).to_be_bytes()).unwrap();
                if !ok { return; }
            }
        }
        if password.is_none() { s.write_all(&0u32.to_be_bytes()).unwrap(); }
        assert_eq!(take(&mut s, 1), [1], "shared session");
        let mut init = vec![0, 4, 0, 2];
        init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
        init.extend_from_slice(&2u32.to_be_bytes());
        init.extend_from_slice(b"vm");
        s.write_all(&init).unwrap();
        assert_eq!(take(&mut s, 20)[0], 0, "SetPixelFormat");
        let encodings = take(&mut s, 4);
        assert_eq!(encodings[0], 2);
        take(&mut s, 4 * u16::from_be_bytes([encodings[2], encodings[3]]) as usize);
        let mut captures = 0;
        loop {
            let mut kind = [0u8; 1];
            if s.read_exact(&mut kind).is_err() { return; }
            match kind[0] {
                3 => {
                    take(&mut s, 9);
                    captures += 1;
                    s.write_all(&[2]).unwrap(); // A bell to skip.
                    if captures == 1 {
                        // Two rectangles: left half red, right half blue (B, G, R, X).
                        let mut update = vec![0, 0, 0, 2];
                        for (x, colour) in [(0u16, [0, 0, 255, 0]), (2, [255, 0, 0, 0])] {
                            for value in [x, 0, 2, 2] { update.extend_from_slice(&value.to_be_bytes()); }
                            update.extend_from_slice(&0i32.to_be_bytes());
                            for _ in 0..4 { update.extend_from_slice(&colour); }
                        }
                        s.write_all(&update).unwrap();
                    } else if captures == 3 {
                        // Nothing changed: like Virtualization.framework, send no update at all.
                        let _ = &s;
                    } else {
                        // The screen grows to 2×3 and is green.
                        let mut update = vec![0, 0, 0, 2];
                        for value in [0u16, 0, 2, 3] { update.extend_from_slice(&value.to_be_bytes()); }
                        update.extend_from_slice(&(-223i32).to_be_bytes());
                        for value in [0u16, 0, 2, 3] { update.extend_from_slice(&value.to_be_bytes()); }
                        update.extend_from_slice(&0i32.to_be_bytes());
                        for _ in 0..6 { update.extend_from_slice(&[0, 255, 0, 0]); }
                        s.write_all(&update).unwrap();
                    }
                }
                4 => { let b = take(&mut s, 7); tx.send(Seen::Key(u32::from_be_bytes([b[3], b[4], b[5], b[6]]), b[0] == 1)).unwrap(); }
                5 => { let b = take(&mut s, 5); tx.send(Seen::Pointer(b[0], u16::from_be_bytes([b[1], b[2]]), u16::from_be_bytes([b[3], b[4]]))).unwrap(); }
                other => panic!("unexpected client message {other}"),
            }
        }
    });
    (address, rx)
}

const WAIT: Duration = Duration::from_secs(5);

#[test]
fn captures_raw_frames_and_follows_a_resize() {
    let (address, _) = serve(None);
    let mut rfb = Rfb::connect(address, None, WAIT).unwrap();
    assert_eq!(rfb.size(), (4, 2));
    let frame = rfb.capture().unwrap();
    assert_eq!(frame.get_pixel(0, 1).0, [255, 0, 0, 255]);
    assert_eq!(frame.get_pixel(3, 0).0, [0, 0, 255, 255]);
    let grown = rfb.capture().unwrap();
    assert_eq!((grown.width(), grown.height()), (2, 3));
    assert_eq!(grown.get_pixel(1, 2).0, [0, 255, 0, 255]);
    // An unchanged screen answers nothing; the client returns the frame it already has, quickly.
    let started = std::time::Instant::now();
    let same = rfb.capture().unwrap();
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    assert_eq!(same, grown);
}

#[test]
fn sends_pointer_and_key_events_clamped_to_the_screen() {
    let (address, seen) = serve(None);
    let mut rfb = Rfb::connect(address, None, WAIT).unwrap();
    rfb.pointer(3, 1, 1).unwrap();
    rfb.pointer(500, 500, 0).unwrap();
    rfb.key(0xff0d, true).unwrap();
    rfb.key(0xff0d, false).unwrap();
    let events: Vec<_> = (0..4).map(|_| seen.recv_timeout(WAIT).unwrap()).collect();
    assert_eq!(events, vec![Seen::Pointer(1, 3, 1), Seen::Pointer(0, 3, 1), Seen::Key(0xff0d, true), Seen::Key(0xff0d, false)]);
}

#[test]
fn authenticates_with_a_vnc_password() {
    let (address, _) = serve(Some("secret"));
    assert_eq!(Rfb::connect(address, Some("secret"), WAIT).unwrap().size(), (4, 2));
    let (address, _) = serve(Some("secret"));
    assert!(Rfb::connect(address, Some("wrong"), WAIT).err().unwrap().contains("rejected"));
    let (address, _) = serve(Some("secret"));
    assert!(Rfb::connect(address, None, WAIT).err().unwrap().contains("password"));
}

/// Against a running VM: `PANDACODE_VNC=vnc://:password@127.0.0.1:port cargo test --lib -- --ignored vm::rfb::tests::live`
#[test]
#[ignore]
fn live_capture_and_type() {
    let url = std::env::var("PANDACODE_VNC").expect("set PANDACODE_VNC");
    let (password, address) = crate::vm::parse_vnc_url(&url).unwrap();
    let mut rfb = Rfb::connect(address, password.as_deref(), WAIT).unwrap();
    let frame = rfb.capture().unwrap();
    let path = std::env::temp_dir().join("pandacode-vm-live.png");
    frame.save(&path).unwrap();
    eprintln!("{}×{} saved to {}", frame.width(), frame.height(), path.display());
    for ch in "echo pandacode".chars() {
        rfb.key(ch as u32, true).unwrap();
        rfb.key(ch as u32, false).unwrap();
    }
    std::thread::sleep(Duration::from_millis(400));
    rfb.capture().unwrap().save(std::env::temp_dir().join("pandacode-vm-live-typed.png")).unwrap();
}
