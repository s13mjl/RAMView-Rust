#![windows_subsystem = "windows"]

mod app;
mod hit;
mod layout;
mod render;
mod search;
mod treemap;
mod ui;
mod viewmodel;
mod worker;

/// NUL-terminated UTF-16 buffer for Win32 calls.
pub fn native_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn main() {
    let _ = app::RamViewApp::run();
}
