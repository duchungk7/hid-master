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
    // 1. 初始化 HidApi
    let api = HidApi::new().map_err(|e| format!("HID API 初始化失敗: {}", e))?;
    let c_path = CString::new(path.clone()).map_err(|_| "路徑格式錯誤")?;

    // 2. 嘗試開啟設備
    let device = match api.open_path(&c_path) {
        Ok(dev) => dev,
        Err(e) => {
            let err_msg = e.to_string();
            if cfg!(target_os = "macos") && err_msg.contains("0xE00002C5") {
                return Err("開啟失敗: macOS 系統正在獨佔此介面。請在前端過濾掉 Usage Page 1 的路徑，選擇 Vendor Defined 介面。".into());
            }
            return Err(format!("開啟失敗: {}", err_msg));
        }
    };

    // 3. 準備寫入數據 (Report ID 處理)
    // 大多數 HID 設備（特別是電競滑鼠自定義協定）需要 65 字節
    // 第一個字節必須是 Report ID (通常為 0x00)
    let mut write_buf = vec![0u8; 65]; 
    let copy_len = std::cmp::min(data.len(), 64);
    
    // 如果數據本身已經包含 Report ID 0x00 且長度正確，直接使用
    // 否則，我們強制將數據放在第 2 個字節開始 (Index 1)
    if data.is_empty() {
        return Err("數據不能為空".into());
    }

    if data[0] == 0x00 && data.len() <= 65 {
        // 前端已補 0x00
        let actual_len = std::cmp::min(data.len(), 65);
        write_buf[..actual_len].copy_from_slice(&data[..actual_len]);
    } else {
        // 前端未補 0x00，我們幫它補在最前面
        write_buf[1..copy_len + 1].copy_from_slice(&data[..copy_len]);
    }

    // 4. 寫入設備
    device.write(&write_buf).map_err(|e| format!("寫入失敗: {}", e))?;

    // 5. 讀取回傳 (增加 macOS 穩定性)
    let mut read_buf = [0u8; 64];
    // 使用 timeout 避免在某些平台永久阻塞
    match device.read_timeout(&mut read_buf, 1000) {
        Ok(res) if res > 0 => {
            // macOS 有時會在讀取結果前頭多加一個 Report ID 0x00，視情況過濾
            Ok(read_buf[..res].to_vec())
        },
        Ok(_) => Ok(Vec::new()), // 超時但無資料
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