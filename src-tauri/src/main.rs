// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]


use tauri::{AppHandle, Emitter, State}; // 導入 Emitter 用於發送事件
use std::thread;
//use std::time::Duration;
use std::collections::HashSet;
use std::sync::Mutex;

// 定義一個全域狀態來管理監聽中的路徑
struct MonitorState(Mutex<HashSet<String>>);

#[tauri::command]
async fn start_listening(
    app: AppHandle, 
    path: String, 
    state: State<'_, MonitorState> // 注入狀態
) -> Result<(), String> {
    let mut listeners = state.0.lock().unwrap();
    
    // 關鍵修正：如果該路徑已經在監聽，直接退出，不開啟新執行緒
    if listeners.contains(&path) {
        println!("路徑 {} 已經在監聽中，跳過啟動。", path);
        return Ok(());
    }

    listeners.insert(path.clone());
    let path_task = path.clone();

    thread::spawn(move || {
        let api = hidapi::HidApi::new().unwrap();
        if let Ok(device) = api.open_path(&std::ffi::CString::new(path_task).unwrap()) {
            loop {
                let mut buf = [0u8; 64];
                match device.read_timeout(&mut buf, 100) {
                    Ok(n) if n > 0 => {
                        let _ = app.emit("hid-data", buf[..n].to_vec());
                    }
                    Err(_) => break, // 裝置斷開則退出
                    _ => (),
                }
            }
        }
    });

    Ok(())
}


use hidapi::{HidApi, DeviceInfo};
use serde::Serialize;
use std::ffi::CString;

#[derive(Serialize)]
struct HidDevice {
    path: String,
    vendor_id: String,
    product_id: String,
    product_string: Option<String>,
    manufacturer_string: Option<String>,
    usage_page: u16,
    interface_number: i32, // Linux 與 macOS 區分介面的重要指標
}


#[tauri::command]
fn scan_hid_devices() -> Result<Vec<HidDevice>, String> {
    // 1. 初始化 API
    let api = HidApi::new().map_err(|e| e.to_string())?;
    
    // 2. 獲取設備清單
    let devices = api.device_list()
        .map(|device: &DeviceInfo| {
            // 處理跨平台路徑 (重要修正)
            // Linux 的路徑可能是 /dev/hidrawX，macOS 是 IOKit 識別碼
            let path_str = device.path().to_string_lossy().to_string();

            HidDevice {
                path: path_str,
                vendor_id: format!("{:#06x}", device.vendor_id()),
                product_id: format!("{:#06x}", device.product_id()),
                product_string: device.product_string().map(|s| s.to_string()),
                manufacturer_string: device.manufacturer_string().map(|s| s.to_string()),
                usage_page: device.usage_page(),
                interface_number: device.interface_number(),
            }
        })
        .collect();

    Ok(devices)
}

// --- 關鍵修正：針對不同作業系統的 Write 邏輯 ---
#[tauri::command]
async fn send_hid_command(path: String, data: Vec<u8>) -> Result<Vec<u8>, String> {
    let api = HidApi::new().map_err(|e| e.to_string())?;
    let c_path = CString::new(path).map_err(|_| "Invalid path format")?;
    
    let device = api.open_path(&c_path)
        .map_err(|e| format!("開啟失敗 (Linux 請檢查 udev 權限): {}", e))?;

    // --- 跨平台 Report ID 處理策略 ---
    // 很多 HID 設備在 Windows 需要 0x00 開頭補齊到 65 Bytes
    // 但某些 Linux 驅動會拒絕大於協定定義長度的數據
    let mut write_data = data;

    // 如果設備是特定類別或是在 macOS/Linux 下，
    // 且前端沒給 Report ID，我們才自動補 0。
    // 這裡我們維持你的邏輯，但增加錯誤捕捉。
    if write_data.len() < 65 && cfg!(target_os = "windows") {
        let mut report = vec![0u8; 65];
        let len = std::cmp::min(write_data.len(), 65);
        report[..len].copy_from_slice(&write_data[..len]);
        write_data = report;
    }

    device.write(&write_data).map_err(|e| format!("寫入失敗: {}", e))?;

    // --- 讀取邏輯優化 ---
    // 在 Linux 下，read 速度很快；在 macOS 下，read 可能會阻塞
    let mut read_buf = [0u8; 64];
    match device.read_timeout(&mut read_buf, 1000) { // 增加到 1s 確保跨平台回傳穩定
        Ok(res) if res > 0 => Ok(read_buf[..res].to_vec()),
        Ok(_) => Ok(Vec::new()),
        Err(e) => Err(format!("讀取異常: {}", e)),
    }
}

fn main() {
    tauri::Builder::default()
        .manage(MonitorState(Mutex::new(HashSet::new()))) // 註冊狀態管理
        .invoke_handler(tauri::generate_handler![scan_hid_devices, send_hid_command, start_listening])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}