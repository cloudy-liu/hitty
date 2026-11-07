/// Settings for the current workspace.
pub struct WorkspaceSettings {
    pub theme: String,
    pub restore_sessions: bool,
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            restore_sessions: true,
        }
    }
}
