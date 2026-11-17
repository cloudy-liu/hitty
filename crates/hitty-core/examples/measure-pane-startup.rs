//! Isolated Windows shell startup measurements through the real pane engine.
//! Run with a JSON options file; this measures daemon-to-editable-prompt,
//! not the sidebar click or the first painted frame.
use hitty_core::{
    core::config::Config,
    daemon::{
        pane::DaemonPane,
        protocol::{DaemonMsg, ShellSpec, WinSize},
    },
};
use serde::Deserialize;
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Options {
    config_dir: PathBuf,
    cwd: PathBuf,
    shell: ShellSpec,
    samples: usize,
    output: PathBuf,
    budget_ms: Option<f64>,
    trace_output: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let options: Options = serde_json::from_slice(&std::fs::read(
        std::env::args_os().nth(1).expect("options.json"),
    )?)?;
    anyhow::ensure!(
        (1..=100).contains(&options.samples),
        "samples must be 1..100"
    );
    #[cfg(windows)]
    {
        let executable = std::env::current_exe()?;
        let directory = executable.parent().expect("executable directory");
        anyhow::ensure!(
            directory.join("conpty.dll").is_file() && directory.join("OpenConsole.exe").is_file(),
            "copy Hitty's bundled conpty.dll and OpenConsole.exe beside the example before measuring"
        );
    }
    hitty_core::core::config::set_config_dir(options.config_dir.clone());
    // Before starting any worker: all derived state belongs to this fixture.
    // Agents launched inside Cmder inherit its initialization flags and aliases.
    // Those would skip initialization and even point at the real user profile.
    let inherited: Vec<_> = std::env::vars_os().map(|(key, _)| key).collect();
    for key in inherited {
        let name = key.to_string_lossy().to_ascii_lowercase();
        if name.starts_with("cmder_")
            || name.starts_with("clink_")
            || name.starts_with("lib_")
            || name.starts_with("print_")
            || [
                "aliases",
                "user_aliases",
                "fast_init",
                "debug_output",
                "verbose_output",
                "git_install_root",
                "hitty_pane_engine",
            ]
            .contains(&name.as_str())
        {
            unsafe {
                std::env::remove_var(key);
            }
        }
    }
    unsafe {
        std::env::set_var("HITTY_DATA_DIR", options.config_dir.join("data"));
    }
    let config = Config::load();
    anyhow::ensure!(
        config.env.get("CMDER_ROOT").is_some(),
        "isolated CMDER_ROOT required"
    );
    let mut results = Vec::new();
    for sample in 0..options.samples {
        let (tx, rx) = mpsc::channel();
        let started = Instant::now();
        let pane = DaemonPane::spawn(
            sample as u64 + 1,
            Some(options.cwd.clone()),
            WinSize {
                cols: 120,
                rows: 30,
                cell_w: 8,
                cell_h: 17,
            },
            Some(options.shell.clone()),
            None,
            None,
            None,
            || {},
        )?;
        let spawned_ms = started.elapsed().as_secs_f64() * 1000.0;
        pane.attach(tx);
        let result = (|| -> anyhow::Result<_> {
            let mut round = 0;
            let mut transcript = Vec::new();
            let mut editable_ms = None;
            let deadline = started + Duration::from_secs(30);
            loop {
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "sample {sample} timed out at round {round}"
                );
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(DaemonMsg::Output(bytes) | DaemonMsg::Snapshot(bytes)) => {
                        let previous = transcript.len();
                        transcript.extend(bytes);
                        // Later Clink DA queries still belong to the emulator.
                        for _ in transcript[previous.saturating_sub(2)..]
                            .windows(3)
                            .filter(|b| *b == b"\x1b[c")
                        {
                            pane.write_input(b"\x1b[?62;c");
                        }
                    }
                    Ok(DaemonMsg::Prompt {
                        active: true,
                        at_prompt: true,
                        ..
                    }) => {
                        if round == 0 {
                            editable_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
                            if let Some(path) = &options.trace_output {
                                std::fs::write(path, &transcript)?;
                            }
                            transcript.clear();
                            pane.write_input(b"hitty-startup-fixtu\t\r");
                            round = 1;
                        } else if transcript.windows(5).any(|b| b == b"73421") {
                            if round == 2 {
                                break;
                            }
                            transcript.clear();
                            pane.write_input(b"\x1b[A\r");
                            round = 2;
                        }
                    }
                    Ok(DaemonMsg::Exited { .. }) => {
                        anyhow::bail!("shell exited during sample {sample}")
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => anyhow::bail!("pane disconnected"),
                    _ => {}
                }
            }
            Ok(serde_json::json!({"sample":sample, "spawned_ms":spawned_ms,
                "editable_ms":editable_ms, "alias_completion_and_history":"passed"}))
        })();
        pane.kill();
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                results.push(serde_json::json!({"sample": sample, "error": error.to_string()}));
                std::fs::write(&options.output, serde_json::to_vec_pretty(&results)?)?;
                return Err(error);
            }
        };
        println!("{result}");
        results.push(result);
    }
    std::fs::write(options.output, serde_json::to_vec_pretty(&results)?)?;
    if let Some(budget) = options.budget_ms {
        anyhow::ensure!(budget.is_finite() && budget > 0.0, "invalid budget_ms");
        let mut timings: Vec<_> = results
            .iter()
            .map(|r| r["editable_ms"].as_f64().unwrap())
            .collect();
        timings.sort_by(f64::total_cmp);
        let p95 = timings[(timings.len() * 95).div_ceil(100) - 1];
        anyhow::ensure!(
            p95 < budget,
            "P95 {p95:.2}ms exceeds {budget:.2}ms budget; results saved"
        );
    }
    Ok(())
}
