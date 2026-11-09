//! A deterministic agent/editor process tree sharing one console input handle.
use std::io::{self, BufRead, Write};
use std::process::Command;

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--editor") {
        println!("EDITOR_READY");
        io::stdout().flush().unwrap();
        let mut line = String::new();
        io::stdin().read_line(&mut line).unwrap();
        return;
    }

    println!("AGENT_READY");
    io::stdout().flush().unwrap();
    for line in io::stdin().lock().lines() {
        match line.unwrap().trim() {
            "ready" => {
                println!("AGENT_READY");
                io::stdout().flush().unwrap();
            }
            "edit" => {
                let editor = std::env::current_exe()
                    .unwrap()
                    .with_file_name("test-editor.exe");
                assert!(
                    Command::new(editor)
                        .arg("--editor")
                        .status()
                        .unwrap()
                        .success()
                );
                // Deliberately silent: the ownership poll must notice the editor
                // returning input even without another terminal-output read.
            }
            "exit" => return,
            other => panic!("unexpected fixture input: {other:?}"),
        }
    }
}
