//! Standalone stdio peer compiled by herdr_engine.rs. No hc installation needed.
use std::io::{self, BufRead, Read, Write};

fn frame(out: &mut impl Write, channel: u32, kind: u8, bytes: &[u8]) {
    out.write_all(&channel.to_le_bytes()).unwrap();
    out.write_all(&[kind]).unwrap();
    out.write_all(&(bytes.len() as u32).to_le_bytes()).unwrap();
    out.write_all(bytes).unwrap();
    out.flush().unwrap();
}

fn main() {
    let mode = std::env::var_os("HITTY_DATA_DIR")
        .and_then(|dir| std::fs::read_to_string(std::path::Path::new(&dir).join("fake-hc-mode")).ok())
        .unwrap_or_default();
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("{}", if mode == "wrong-binary" { "another-program 1.0" } else { "hc 0.1.15" });
        return;
    }
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut hello = String::new();
    input.read_line(&mut hello).unwrap();
    assert!(hello.contains("\"hello\""));
    let generation = if mode == "generation" { 2 } else { 1 };
    let version = if mode == "old-version" { "0.1.14" } else { "0.1.15" };
    let methods = if mode == "missing-methods" { "[]" } else {
        r#"["terminal.run","terminal.attach","terminal.get","terminal.list","terminal.kill","terminal.rm","events.subscribe"]"#
    };
    writeln!(output, "{{\"id\":0,\"result\":{{\"generation\":{generation},\"version\":\"{version}\",\"framed\":true,\"methods\":{methods}}}}}").unwrap();
    output.flush().unwrap();
    let terminal = r#"{"address":"t1","pid":null,"state":"live","labels":{}}"#;
    loop {
        let mut header = [0; 9];
        if input.read_exact(&mut header).is_err() { break; }
        let channel = u32::from_le_bytes(header[..4].try_into().unwrap());
        let kind = header[4];
        let len = u32::from_le_bytes(header[5..].try_into().unwrap()) as usize;
        assert!(len < 4 * 1024 * 1024);
        let mut bytes = vec![0; len];
        input.read_exact(&mut bytes).unwrap();
        if channel == 0 {
            // Only the request id needs parsing: this scripted peer has fixed replies.
            let text = String::from_utf8(bytes).unwrap();
            let id: String = text.split("\"id\":").nth(1).unwrap()
                .trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
            let result = if text.contains("\"events.subscribe\"") {
                r#"{"seq":0,"channel":20,"terminals":[]}"#.to_owned()
            } else if text.contains("\"terminal.attach\"") {
                frame(&mut output, 5, 0, b"FAKE_HC_READY\r\n"); // before reply
                format!("{{\"channel\":5,\"terminal\":{terminal}}}")
            } else if text.contains("\"terminal.run\"") || text.contains("\"terminal.get\"") {
                terminal.to_owned()
            } else { "{}".to_owned() };
            frame(&mut output, 0, 0, format!("{{\"id\":{id},\"result\":{result}}}").as_bytes());
        } else if channel == 5 {
            match kind {
                0 if bytes == b"exit\r" => frame(&mut output, 5, 2, br#"{"exit_code":7}"#),
                0 => frame(&mut output, 5, 0, &bytes),
                3 => {
                    let cols = u16::from_le_bytes(bytes[..2].try_into().unwrap());
                    let rows = u16::from_le_bytes(bytes[2..4].try_into().unwrap());
                    frame(&mut output, 5, 0, format!("FAKE_RESIZE:{cols}x{rows}").as_bytes());
                }
                _ => {}
            }
        }
    }
}
