//! Identify the conversation a live agent has open, without choosing a recent
//! file from the workspace. Hooks and explicit resume arguments cover the
//! general case. Agents that keep a conversation file open can also be
//! identified by the daemon, without a command running on each turn.

use std::path::{Path, PathBuf};

use crate::core::cli_agent::CLIAgent;

pub(super) enum LiveSession {
    Saved {
        id: String,
        config_dir: Option<PathBuf>,
    },
    Unsaved(String),
}

pub(super) fn probe(
    agent: CLIAgent,
    shell_pid: u32,
    foreground: Option<i32>,
) -> Option<LiveSession> {
    let identify: fn(&[PathBuf], u32) -> Option<LiveSession> = match agent {
        CLIAgent::Codex => |files, _| {
            codex_session(files).map(|id| LiveSession::Saved {
                id,
                config_dir: None,
            })
        },
        CLIAgent::QoderCLI | CLIAgent::QoderCLICn => qoder_session,
        _ => return None,
    };
    let mut processes = super::procinfo::processes(shell_pid, foreground)
        .into_iter()
        .filter(|process| {
            #[cfg(windows)]
            if agent == CLIAgent::Codex && super::codex_launch::is_launcher(process.pid) {
                return false;
            }
            CLIAgent::detect_from_argv(std::slice::from_ref(&process.name)) == Some(agent)
        });
    let process = processes.next()?;
    // Nested agents of the same kind can belong to different conversations.
    if processes.next().is_some() {
        return None;
    }
    identify(&super::procinfo::open_files(process.pid), process.pid)
}

fn uuid(value: &str) -> Option<&str> {
    (value.len() == 36 && uuid::Uuid::try_parse(value).is_ok()).then_some(value)
}

fn rollout_id(path: &Path) -> Option<&str> {
    let name = path
        .file_name()?
        .to_str()?
        .strip_prefix("rollout-")?
        .strip_suffix(".jsonl")?;
    let id = name.get(name.len().checked_sub(36)?..)?;
    (path.ancestors().nth(4)?.file_name()? == "sessions").then_some(())?;
    uuid(id)
}

fn codex_session(paths: &[PathBuf]) -> Option<String> {
    let mut rollouts = std::collections::HashSet::new();
    let mut locks = std::collections::HashSet::new();
    for path in paths {
        if let Some(id) = rollout_id(path) {
            rollouts.insert(id);
        }
        if path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "thread-writer-locks")
            && let Some(id) = path
                .file_stem()
                .and_then(|name| name.to_str())
                .and_then(uuid)
        {
            locks.insert(id);
        }
    }
    // More than one writer can mean Codex subagents sharing the process.
    // A lock without a rollout can be a new, empty thread, not resumable history.
    if rollouts.len() != 1 || locks.len() > 1 {
        return None;
    }
    let id = *rollouts.iter().next()?;
    (locks.is_empty() || locks.contains(id)).then(|| id.to_string())
}

fn qoder_session(paths: &[PathBuf], pid: u32) -> Option<LiveSession> {
    // Qoder keeps the active diagnostic segment open even when idle. Its
    // name includes the owning PID and its parent directories identify the
    // project and session. The diagnostic log exists before the transcript.
    let suffix = format!("-p{pid}.jsonl");
    let mut transcripts = paths
        .iter()
        .filter_map(|path| qoder_transcript(path, &suffix));
    let (id, config_dir, transcript) = transcripts.next()?;
    if transcripts.any(|(other_id, _, other_path)| other_id != id || other_path != transcript) {
        return None;
    }
    match transcript.metadata() {
        Ok(meta) if meta.is_file() && meta.len() != 0 => Some(LiveSession::Saved {
            id: id.into(),
            config_dir: Some(config_dir.into()),
        }),
        Ok(_) => Some(LiveSession::Unsaved(id.into())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Some(LiveSession::Unsaved(id.into()))
        }
        Err(_) => None,
    }
}

fn qoder_transcript<'a>(segment: &'a Path, suffix: &str) -> Option<(&'a str, &'a Path, PathBuf)> {
    let run = segment.file_name()?.to_str()?.strip_suffix(suffix)?;
    let segments = segment.parent()?;
    let session = segments.parent()?;
    let project = session.parent()?;
    let sessions = project.parent()?;
    let logs = sessions.parent()?;
    if run.is_empty()
        || segments.file_name()? != "segments"
        || sessions.file_name()? != "sessions"
        || logs.file_name()? != "logs"
    {
        return None;
    }
    let id = uuid(session.file_name()?.to_str()?)?;
    let config_dir = logs.parent()?;
    let transcript = config_dir
        .join("projects")
        .join(project.file_name()?)
        .join(format!("{id}.jsonl"));
    Some((id, config_dir, transcript))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn windows_reads_files_held_by_the_requested_process() {
        let dir = tempfile::tempdir().unwrap();
        let id = "0199c3f2-1b0e-7c3a-9f21-6d4b8e2a5c17";
        let directory = dir.path().join("sessions/2026/09/13");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("rollout-2026-09-13T15-00-00-{id}.jsonl"));
        let _open = std::fs::File::create(&path).unwrap();
        let path = std::fs::canonicalize(path).unwrap();
        let files: Vec<_> = super::super::procinfo::open_files(std::process::id())
            .into_iter()
            .filter(|open| open == &path)
            .collect();
        assert_eq!(codex_session(&files).as_deref(), Some(id));
    }

    #[test]
    fn only_the_processes_open_conversation_is_a_resume_target() {
        let id = "0199c3f2-1b0e-7c3a-9f21-6d4b8e2a5c17";
        let other = "0199c3f2-1b0e-7c3a-9f21-6d4b8e2a5c18";
        let rollout = |id| {
            PathBuf::from(format!(
                "profile/sessions/2026/09/13/rollout-2026-09-13T15-00-00-{id}.jsonl"
            ))
        };
        let lock = |id| PathBuf::from(format!("profile/thread-writer-locks/{id}.lock"));
        assert_eq!(codex_session(&[rollout(id), lock(id)]).as_deref(), Some(id));
        assert_eq!(codex_session(&[rollout(id)]).as_deref(), Some(id));
        assert_eq!(
            codex_session(&[lock(id)]),
            None,
            "an empty launch has no history"
        );
        assert_eq!(
            codex_session(&[rollout(id), rollout(other), lock(id)]),
            None
        );
        assert_eq!(codex_session(&[rollout(id), lock(other)]), None);
        assert_eq!(codex_session(&[rollout(id), lock(id), lock(other)]), None);
        assert_eq!(
            codex_session(&[PathBuf::from(format!("notes/rollout-{id}.jsonl"))]),
            None
        );
    }

    #[test]
    fn qoder_identifies_the_open_session_instead_of_other_history() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = "0199c3f2-1b0e-7c3a-9f21-6d4b8e2a5c17";
        let other = "0199c3f2-1b0e-7c3a-9f21-6d4b8e2a5c18";
        let segment = |id, pid| {
            root.join(format!(
                "logs/sessions/project/{id}/segments/run-p{pid}.jsonl"
            ))
        };
        let project = root.join("projects/project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join(format!("{other}.jsonl")), "{}\n").unwrap();
        let history = project.join(format!("{id}.jsonl"));
        let files = [segment(id, 42), segment(other, 43)];
        assert!(matches!(
            qoder_session(&files, 42),
            Some(LiveSession::Unsaved(active)) if active == id
        ));
        std::fs::write(&history, "").unwrap();
        assert!(matches!(
            qoder_session(&files, 42),
            Some(LiveSession::Unsaved(_))
        ));
        std::fs::write(history, "{}\n").unwrap();
        assert!(matches!(
            qoder_session(&files, 42),
            Some(LiveSession::Saved { id: active, config_dir: Some(dir) }) if active == id && dir == root
        ));
        assert!(qoder_session(&files, 4).is_none());
        assert!(qoder_session(&[files[0].clone(), segment(other, 42)], 42).is_none());
    }
}
