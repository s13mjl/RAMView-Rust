use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
};

use crate::native::{pcwstr, wide};

const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APP_NAME: &str = "RAMView";

/// Manages Windows startup registration (§31) for unpackaged desktop deployment.
pub struct StartupManager;

impl StartupManager {
    pub fn is_startup_enabled() -> bool {
        unsafe {
            let sub_key = wide(RUN_KEY_PATH);
            let mut key = HKEY::default();

            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                pcwstr(&sub_key),
                0,
                KEY_QUERY_VALUE,
                &mut key,
            ) != ERROR_SUCCESS
            {
                return false;
            }

            let value_name = wide(APP_NAME);
            let mut value_type = Default::default();
            let mut size = 0u32;

            let result = RegQueryValueExW(
                key,
                pcwstr(&value_name),
                None,
                Some(&mut value_type),
                None,
                Some(&mut size),
            );

            let _ = RegCloseKey(key);
            result == ERROR_SUCCESS
        }
    }

    pub fn set_startup(enable: bool) -> bool {
        unsafe {
            let sub_key = wide(RUN_KEY_PATH);
            let mut key = HKEY::default();

            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                pcwstr(&sub_key),
                0,
                KEY_SET_VALUE,
                &mut key,
            ) != ERROR_SUCCESS
            {
                return false;
            }

            let value_name = wide(APP_NAME);

            if enable {
                let exe_path = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.to_str().map(|s| s.to_string()))
                    .unwrap_or_default();

                if exe_path.is_empty() || !std::path::Path::new(&exe_path).is_file() {
                    let _ = RegCloseKey(key);
                    return false;
                }

                let data = format!("\"{exe_path}\" --minimized");
                let bytes: Vec<u8> = data
                    .encode_utf16()
                    .flat_map(|unit| unit.to_ne_bytes())
                    .chain([0u8, 0u8])
                    .collect();

                let result = RegSetValueExW(key, pcwstr(&value_name), 0, REG_SZ, Some(&bytes));
                let _ = RegCloseKey(key);
                result == ERROR_SUCCESS
            } else {
                let result = RegDeleteValueW(key, pcwstr(&value_name));
                let _ = RegCloseKey(key);
                // Missing values are still a successful "disabled" state.
                result == ERROR_SUCCESS
                    || result == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND
            }
        }
    }
}
