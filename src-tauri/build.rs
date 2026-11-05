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
}
