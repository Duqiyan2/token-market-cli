// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if tokenmarket_core::connections::qwenwork_relay::is_invocation() {
        if tokenmarket_core::connections::qwenwork_relay::run_blocking().is_err() {
            std::process::exit(1);
        }
        return;
    }
    app_lib::run();
}
