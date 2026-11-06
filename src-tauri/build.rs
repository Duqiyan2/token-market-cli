fn main() {
    let mut attributes = tauri_build::Attributes::new();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        // Lib-test executables and examples do not inherit the application's
        // resource manifest. Without Common Controls v6, TaskDialogIndirect
        // fails to resolve before the test harness or verifier can start.
        // Use the linker for all targets, as Tauri does for its own tests:
        // https://github.com/tauri-apps/tauri/pull/4383#issuecomment-1212221864
        // Omit the resource copy so the application has only one manifest.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(attributes).expect("failed to run tauri-build");

    // Tauri's generated runtime Config deliberately omits bundle.windows.nsis.
    // Read the same target config and CLI merge used by tauri-build instead of
    // treating the missing runtime field as an unsupported installation.
    let target = tauri_utils::platform::Target::from_triple(&std::env::var("TARGET").unwrap());
    let (mut config, _) =
        tauri_utils::config::parse::read_from(target, &std::env::current_dir().unwrap())
            .expect("failed to read update installation policy");
    if let Ok(overrides) = std::env::var("TAURI_CONFIG") {
        json_patch::merge(
            &mut config,
            &serde_json::from_str(&overrides).expect("invalid TAURI_CONFIG"),
        );
    }
    let config: tauri_utils::config::Config =
        serde_json::from_value(config).expect("invalid update installation policy");
    let current_user_nsis = config.bundle.windows.nsis.as_ref().is_some_and(|nsis| {
        nsis.install_mode == tauri_utils::config::NSISInstallerMode::CurrentUser
            && nsis.template.is_none()
    });
    println!("cargo:rustc-env=TOKEN_MARKET_CURRENT_USER_NSIS={current_user_nsis}");
}
