use anyhow::Result;
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};
use tokenmarket_core::{
    installation::{InstallExecutor, InstallStep, InstallationManager},
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};

struct Metadata(&'static str);
impl InstallExecutor for Metadata {
    fn execute(&self, _: &InstallStep, _: &AtomicBool, _: &mut dyn FnMut(String)) -> Result<()> {
        panic!("compatibility detection must not install or launch DuMate")
    }
    fn version(&self, _: &Path) -> Result<String> {
        panic!("must not run the application")
    }
    fn application_version(&self, _: &Path) -> Result<Option<String>> {
        Ok(Some(self.0.into()))
    }
}

#[test]
fn dumate_connection_is_not_gated_by_installer_version() {
    for version in ["1.0.89.0", "1.0.88", "1.0.90", "2.0.0"] {
        let home = tempfile::tempdir().unwrap();
        let app = home.path().join("Applications/DuMate/DuMate.exe");
        std::fs::create_dir_all(app.parent().unwrap()).unwrap();
        std::fs::write(&app, b"fixture; never execute").unwrap();
        let manager = InstallationManager::new(
            InstallationEnvironment {
                platform: HostPlatform::Windows,
                home: home.path().into(),
                application_directories: vec![home.path().join("Applications")],
                command_directories: vec![],
                executable_extensions: vec![".EXE".into()],
            },
            "x86_64".into(),
            Arc::new(Metadata(version)),
        );
        for tool in [ToolId::DuMate, ToolId::DuMateEnterprise] {
            manager
                .connection_guard(tool)
                .unwrap_or_else(|error| panic!("{tool} {version}: {error}"));
        }
    }
}
