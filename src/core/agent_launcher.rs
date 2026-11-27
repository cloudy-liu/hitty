use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::core::cli_agent::CLIAgent;
use crate::core::config::Config;

/// An installed CLI agent detected on the local `PATH`, paired with the safe
/// default command used when the user has not saved a custom override.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedAgent {
    pub agent: CLIAgent,
    pub default_command: String,
}

const PRIORITY_AGENTS: [CLIAgent; 6] = [
    CLIAgent::Claude,
    CLIAgent::Codex,
    CLIAgent::Antigravity,
    CLIAgent::QoderCLI,
    CLIAgent::QoderCLICn,
    CLIAgent::CodeBuddy,
];

/// Canonical display order for agent launch menus and Settings rows: common
/// primary agents first, followed by remaining agents alphabetically.
pub fn ordered_cli_agents() -> Vec<CLIAgent> {
    let mut agents: Vec<CLIAgent> = CLIAgent::ALL.into_iter().collect();
    agents.sort_by_key(|agent| {
        (
            PRIORITY_AGENTS
                .iter()
                .position(|first| first == agent)
                .unwrap_or(usize::MAX),
            agent.display_name().to_ascii_lowercase(),
        )
    });
    agents
}

/// Interactive executable names checked on `PATH` for launching an agent.
///
/// Omits non-interactive protocol shims (`vibe-acp`) and short aliases that
/// collide with unrelated tools (`cbc` for COIN-OR) so PATH discovery does not
/// surface false positives in the `+` launch menu.
fn launch_aliases(agent: CLIAgent) -> &'static [&'static str] {
    match agent {
        CLIAgent::CodeBuddy => &["codebuddy", "codebuddy-code"],
        CLIAgent::Vibe => &["vibe"],
        other => other.aliases(),
    }
}

/// Safe bare command used to start `agent` interactively in a fresh shell when
/// no custom override is configured.
pub fn default_command_for(agent: CLIAgent, matched_alias: Option<&str>) -> String {
    match agent {
        CLIAgent::Goose => "goose session".to_string(),
        CLIAgent::Hermes => "hermes chat".to_string(),
        _ => matched_alias
            .unwrap_or_else(|| launch_aliases(agent)[0])
            .to_string(),
    }
}

/// Resolves the command executed when launching `agent` from a directory's `+`
/// menu: a trimmed non-empty entry in `config.agent_launch_commands` overrides
/// `default_command`, and an empty or missing entry falls back to the default.
pub fn effective_launch_command(
    agent: CLIAgent,
    default_command: &str,
    config: &Config,
) -> String {
    config
        .agent_launch_commands
        .get(agent.slug())
        .map(|cmd| cmd.trim())
        .filter(|cmd| !cmd.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_command.to_string())
}

/// Agents shown in `Settings -> Agents -> Quick launch commands`: every
/// locally detected agent, plus any agent with a saved custom override so the
/// user can still inspect or clear overrides for agents not currently on PATH.
pub fn settings_launch_agents(detected: &[DetectedAgent], config: &Config) -> Vec<DetectedAgent> {
    let mut out = detected.to_vec();
    let mut extras: Vec<CLIAgent> = config
        .agent_launch_commands
        .keys()
        .filter_map(|slug| CLIAgent::from_slug(slug))
        .filter(|agent| !out.iter().any(|entry| entry.agent == *agent))
        .collect();
    let order = ordered_cli_agents();
    extras.sort_by_key(|agent| {
        order
            .iter()
            .position(|candidate| candidate == agent)
            .unwrap_or(usize::MAX)
    });
    for agent in extras {
        out.push(DetectedAgent {
            agent,
            default_command: default_command_for(agent, None),
        });
    }
    out
}

/// Scans the local process `PATH` (and `PATHEXT` on Windows) for installed CLI
/// agents. Safe to invoke on a background executor.
pub fn detect_installed_agents() -> Vec<DetectedAgent> {
    let Some(path_var) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let path_dirs: Vec<PathBuf> = std::env::split_paths(&path_var)
        .filter(|dir| !dir.as_os_str().is_empty())
        .collect();
    let pathext = current_pathext();
    detect_installed_agents_in(&path_dirs, pathext.as_deref())
}

fn current_pathext() -> Option<Vec<String>> {
    if cfg!(windows) {
        let exts = std::env::var_os("PATHEXT")
            .and_then(|val| parse_pathext(&val))
            .unwrap_or_else(default_windows_pathext);
        Some(exts)
    } else {
        None
    }
}

fn default_windows_pathext() -> Vec<String> {
    vec![
        ".COM".to_string(),
        ".EXE".to_string(),
        ".BAT".to_string(),
        ".CMD".to_string(),
    ]
}

fn parse_pathext(raw: &OsStr) -> Option<Vec<String>> {
    let exts: Vec<String> = raw
        .to_string_lossy()
        .split(';')
        .map(str::trim)
        .filter(|ext| !ext.is_empty())
        .map(|ext| {
            if ext.starts_with('.') {
                ext.to_string()
            } else {
                format!(".{ext}")
            }
        })
        .collect();
    (!exts.is_empty()).then_some(exts)
}

/// Pure, testable PATH discovery over explicit directories and optional
/// Windows `PATHEXT` extensions.
pub fn detect_installed_agents_in(
    path_dirs: &[PathBuf],
    pathext: Option<&[String]>,
) -> Vec<DetectedAgent> {
    let mut found = Vec::new();
    for agent in ordered_cli_agents() {
        if let Some(alias) = find_agent_alias(agent, path_dirs, pathext) {
            found.push(DetectedAgent {
                agent,
                default_command: default_command_for(agent, Some(alias)),
            });
        }
    }
    found
}

fn find_agent_alias(
    agent: CLIAgent,
    path_dirs: &[PathBuf],
    pathext: Option<&[String]>,
) -> Option<&'static str> {
    for &alias in launch_aliases(agent) {
        if path_dirs
            .iter()
            .any(|dir| alias_exists_in_dir(dir, alias, pathext))
        {
            return Some(alias);
        }
    }
    None
}

fn alias_exists_in_dir(dir: &Path, alias: &str, pathext: Option<&[String]>) -> bool {
    match pathext {
        Some(exts) => exts.iter().any(|ext| {
            let candidate = dir.join(format!("{alias}{ext}"));
            if candidate.is_file() {
                return true;
            }
            let lower_ext = ext.to_ascii_lowercase();
            lower_ext != *ext && dir.join(format!("{alias}{lower_ext}")).is_file()
        }),
        None => is_executable_file(&dir.join(alias)),
    }
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && (meta.permissions().mode() & 0o111 != 0))
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_file(dir: &Path, name: &str) {
        let path = dir.join(name);
        std::fs::write(&path, "").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).unwrap();
        }
    }

    #[test]
    fn detects_agents_in_priority_order_with_windows_pathext() {
        let temp = tempfile::tempdir().unwrap();
        let dir1 = temp.path().join("bin1");
        let dir2 = temp.path().join("bin2");
        std::fs::create_dir_all(&dir1).unwrap();
        std::fs::create_dir_all(&dir2).unwrap();

        create_file(&dir1, "aider.exe");
        create_file(&dir1, "codex.cmd");
        create_file(&dir2, "claude.exe");
        create_file(&dir2, "agy.cmd");
        create_file(&dir2, "goose.exe");
        create_file(&dir2, "hermes.bat");
        // Colliding or non-interactive binaries must be ignored.
        create_file(&dir2, "cbc.exe");
        create_file(&dir2, "vibe-acp.exe");

        let exts = default_windows_pathext();
        let detected = detect_installed_agents_in(&[dir1, dir2], Some(&exts));

        assert_eq!(
            detected,
            vec![
                DetectedAgent {
                    agent: CLIAgent::Claude,
                    default_command: "claude".into(),
                },
                DetectedAgent {
                    agent: CLIAgent::Codex,
                    default_command: "codex".into(),
                },
                DetectedAgent {
                    agent: CLIAgent::Antigravity,
                    default_command: "agy".into(),
                },
                DetectedAgent {
                    agent: CLIAgent::Aider,
                    default_command: "aider".into(),
                },
                DetectedAgent {
                    agent: CLIAgent::Goose,
                    default_command: "goose session".into(),
                },
                DetectedAgent {
                    agent: CLIAgent::Hermes,
                    default_command: "hermes chat".into(),
                },
            ]
        );
    }

    #[test]
    fn uses_matched_secondary_alias_as_default_command() {
        let temp = tempfile::tempdir().unwrap();
        create_file(temp.path(), "claude-code.cmd");
        let exts = default_windows_pathext();
        let detected = detect_installed_agents_in(&[temp.path().to_path_buf()], Some(&exts));
        assert_eq!(
            detected,
            vec![DetectedAgent {
                agent: CLIAgent::Claude,
                default_command: "claude-code".into(),
            }]
        );
    }

    #[test]
    fn effective_launch_command_prefers_trimmed_override_and_falls_back_to_default() {
        let mut cfg = Config::default();
        assert_eq!(
            effective_launch_command(CLIAgent::Codex, "codex", &cfg),
            "codex"
        );

        cfg.agent_launch_commands
            .insert("codex".into(), "  codex --yolo  ".into());
        assert_eq!(
            effective_launch_command(CLIAgent::Codex, "codex", &cfg),
            "codex --yolo"
        );

        cfg.agent_launch_commands
            .insert("codex".into(), "   ".into());
        assert_eq!(
            effective_launch_command(CLIAgent::Codex, "codex", &cfg),
            "codex"
        );
    }

    #[test]
    fn settings_launch_agents_includes_saved_overrides_for_undetected_agents() {
        let detected = vec![DetectedAgent {
            agent: CLIAgent::Claude,
            default_command: "claude".into(),
        }];
        let mut cfg = Config::default();
        cfg.agent_launch_commands
            .insert("codex".into(), "codex --yolo".into());
        cfg.agent_launch_commands
            .insert("unknown-agent".into(), "noop".into());

        let rows = settings_launch_agents(&detected, &cfg);
        assert_eq!(
            rows,
            vec![
                DetectedAgent {
                    agent: CLIAgent::Claude,
                    default_command: "claude".into(),
                },
                DetectedAgent {
                    agent: CLIAgent::Codex,
                    default_command: "codex".into(),
                },
            ]
        );
    }
}
