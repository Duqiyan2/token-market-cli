use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
struct Preferences {
    run_in_background: bool,
    close_notice_seen: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            run_in_background: true,
            close_notice_seen: false,
        }
    }
}

struct StoredPreferences {
    value: Preferences,
    error: Option<String>,
}

pub struct WindowLifecycle {
    path: PathBuf,
    preferences: Mutex<StoredPreferences>,
    background_available: AtomicBool,
    close_notice_pending: AtomicBool,
}

#[derive(Debug, PartialEq)]
enum CloseAction {
    Exit,
    Explain,
    Hide,
}

impl WindowLifecycle {
    fn load(path: PathBuf) -> Self {
        let loaded = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| error.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Preferences::default())
            }
            Err(error) => Err(error.to_string()),
        };
        let preferences = match loaded {
            Ok(value) => StoredPreferences { value, error: None },
            Err(error) => StoredPreferences {
                // A corrupt/unreadable preference must not silently override a saved opt-out.
                value: Preferences {
                    run_in_background: false,
                    ..Preferences::default()
                },
                error: Some(format!("无法读取应用设置，请检查并重新保存：{error}")),
            },
        };
        Self {
            path,
            preferences: Mutex::new(preferences),
            background_available: AtomicBool::new(false),
            close_notice_pending: AtomicBool::new(false),
        }
    }

    fn update(&self, change: impl FnOnce(&mut Preferences)) -> Result<(), String> {
        let mut stored = self.preferences.lock().expect("desktop preferences lock");
        let mut next = stored.value.clone();
        change(&mut next);
        let temporary = self.path.with_extension("json.tmp");
        tokenmarket_core::config::write_private_json(&temporary, &next)
            .and_then(|()| fs::rename(&temporary, &self.path).map_err(Into::into))
            .map_err(|error| format!("无法保存应用设置：{error}"))?;
        stored.value = next;
        stored.error = None;
        Ok(())
    }

    fn close_action(&self) -> CloseAction {
        let stored = self.preferences.lock().expect("desktop preferences lock");
        if !self.background_available.load(Ordering::SeqCst) || !stored.value.run_in_background {
            CloseAction::Exit
        } else if !stored.value.close_notice_seen {
            CloseAction::Explain
        } else {
            CloseAction::Hide
        }
    }

    fn value(&self) -> serde_json::Value {
        let stored = self.preferences.lock().expect("desktop preferences lock");
        serde_json::json!({
            "run_in_background": stored.value.run_in_background,
            "background_available": self.background_available.load(Ordering::SeqCst),
            "close_notice_pending": self.close_notice_pending.load(Ordering::SeqCst),
            "error": stored.error,
        })
    }
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("desktop-check-tool-updates", ());
    }
}

pub fn setup(app: &AppHandle) {
    app.manage(WindowLifecycle::load(
        tokenmarket_core::config::token_market_home().join("desktop-preferences.json"),
    ));
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    match create_tray(app) {
        Ok(()) => app
            .state::<WindowLifecycle>()
            .background_available
            .store(true, Ordering::SeqCst),
        Err(error) => eprintln!("Unable to initialize the system tray: {error}"),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    use tauri::{
        menu::{Menu, MenuItem, PredefinedMenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };
    let show = MenuItem::with_id(app, "desktop-show", "打开 Token Market", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "desktop-settings", "应用设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "desktop-quit", "退出应用", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &settings, &separator, &quit])?;
    let mut tray = TrayIconBuilder::with_id("tokenmarket")
        .tooltip("Token Market")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "desktop-show" => show_main_window(app),
            "desktop-settings" => {
                show_main_window(app);
                let _ = app.emit_to("main", "desktop-show-preferences", ());
            }
            "desktop-quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    #[cfg(target_os = "macos")]
    {
        // A transparent 18 pt @2x mask, tinted by macOS for the menu bar appearance.
        // Keep this separate from the colored Dock icon and independent of app theme.
        let icon = tauri::image::Image::new_owned(
            include_bytes!("../icons/tray-template.rgba").to_vec(),
            36,
            36,
        );
        tray = tray.icon(icon).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(icon) = app.default_window_icon() {
            tray = tray.icon(icon.clone());
        }
    }
    tray.build(app)?;
    Ok(())
}

pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != "main" {
        return;
    }
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        let Some(state) = window.try_state::<WindowLifecycle>() else {
            return;
        };
        match state.close_action() {
            CloseAction::Exit => {} // Preserve native closing and system shutdown semantics.
            CloseAction::Explain => {
                api.prevent_close();
                state.close_notice_pending.store(true, Ordering::SeqCst);
                let _ = window.emit("desktop-close-notice", ());
            }
            CloseAction::Hide => {
                api.prevent_close();
                if let Err(error) = window.hide() {
                    eprintln!("Unable to hide the main window: {error}");
                }
            }
        }
    }
}

#[tauri::command]
pub fn desktop_window_preferences(app: AppHandle) -> serde_json::Value {
    app.state::<WindowLifecycle>().value()
}

#[tauri::command]
pub fn save_desktop_window_preferences(
    app: AppHandle,
    run_in_background: bool,
) -> Result<serde_json::Value, String> {
    let state = app.state::<WindowLifecycle>();
    state.update(|value| value.run_in_background = run_in_background)?;
    Ok(state.value())
}

#[tauri::command]
pub fn acknowledge_desktop_close(app: AppHandle) -> Result<(), String> {
    let state = app.state::<WindowLifecycle>();
    if !state.close_notice_pending.load(Ordering::SeqCst)
        || state.close_action() == CloseAction::Exit
    {
        return Err("关闭行为已改变，请重新关闭窗口".into());
    }
    state.update(|value| value.close_notice_seen = true)?;
    app.get_webview_window("main")
        .ok_or("主窗口不可用")?
        .hide()
        .map_err(|error| error.to_string())?;
    state.close_notice_pending.store(false, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn cancel_desktop_close(app: AppHandle) {
    app.state::<WindowLifecycle>()
        .close_notice_pending
        .store(false, Ordering::SeqCst);
}

#[tauri::command]
pub fn quit_desktop_app(app: AppHandle) {
    // Explicit quit bypasses window-close interception and does not log out.
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_path() -> PathBuf {
        static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir()
            .join(format!(
                "tokenmarket-window-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::SeqCst)
            ))
            .join("desktop-preferences.json")
    }

    #[test]
    fn first_close_explains_then_hides_and_opt_out_survives_restart() {
        let path = temporary_path();
        let state = WindowLifecycle::load(path.clone());
        assert_eq!(state.close_action(), CloseAction::Exit); // No usable tray yet.
        state.background_available.store(true, Ordering::SeqCst);
        assert_eq!(state.close_action(), CloseAction::Explain);
        state
            .update(|value| value.close_notice_seen = true)
            .unwrap();
        assert_eq!(state.close_action(), CloseAction::Hide);
        state
            .update(|value| value.run_in_background = false)
            .unwrap();
        let restored = WindowLifecycle::load(path.clone());
        restored.background_available.store(true, Ordering::SeqCst);
        assert_eq!(restored.close_action(), CloseAction::Exit);
        restored
            .update(|value| value.run_in_background = true)
            .unwrap();
        assert_eq!(restored.close_action(), CloseAction::Hide);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn failed_save_does_not_change_active_preferences() {
        let path = temporary_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // A directory where the temporary file should be makes saving fail on every OS.
        fs::create_dir(path.with_extension("json.tmp")).unwrap();
        let state = WindowLifecycle::load(path.clone());
        state.background_available.store(true, Ordering::SeqCst);
        assert!(state
            .update(|value| value.run_in_background = false)
            .is_err());
        assert_eq!(state.close_action(), CloseAction::Explain);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn corrupt_preferences_do_not_silently_enable_background_running() {
        let path = temporary_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "invalid json").unwrap();
        let state = WindowLifecycle::load(path.clone());
        state.background_available.store(true, Ordering::SeqCst);
        assert_eq!(state.close_action(), CloseAction::Exit);
        assert!(state.value()["error"].is_string());
        assert_eq!(fs::read_to_string(&path).unwrap(), "invalid json");
        state
            .update(|value| value.run_in_background = true)
            .unwrap();
        assert!(state.value()["error"].is_null());
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
