#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use hidapi::{HidApi, HidDevice};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State, Manager};

// --- 資料結構 ---

#[derive(Serialize, Clone)]
struct HidDeviceNotify {
    path: String,
    vendor_id: String,
    product_id: String,
    usage_page: u16,
    interface_number: i32,
}

struct ManagedDevice {
    device: Arc<Mutex<HidDevice>>,
    is_paused: Arc<AtomicBool>,
    should_stop: Arc<AtomicBool>,
}

// 管理所有開啟中的設備
struct DeviceManager(Mutex<HashMap<String, ManagedDevice>>);

// --- Helpers ---

fn get_api() -> Result<HidApi, String> {
    HidApi::new().map_err(|e| e.to_string())
}

// --- Commands ---

#[tauri::command]
fn scan_hid_devices() -> Result<Vec<HidDeviceNotify>, String> {
    let api = get_api()?;
    Ok(api.device_list()
        .filter(|d| {
            // macOS 核心過濾：只顯示非系統佔用介面
            if cfg!(target_os = "macos") { d.usage_page() != 0x0001 } else { true }
        })
        .map(|d| HidDeviceNotify {
            path: d.path().to_string_lossy().to_string(),
            vendor_id: format!("{:#06x}", d.vendor_id()),
            product_id: format!("{:#06x}", d.product_id()),
            usage_page: d.usage_page(),
            interface_number: d.interface_number(),
        })
        .collect())
}

#[tauri::command]
async fn start_listening(
    app: AppHandle, 
    path: String, 
    manager_state: State<'_, DeviceManager>
) -> Result<(), String> {
    let mut manager = manager_state.0.lock().unwrap();

    // 如果已經在監聽，就不重複開啟
    if manager.contains_key(&path) { return Ok(()); }

    let api = get_api()?;
    let device_info = api.device_list()
        .find(|d| d.path().to_string_lossy() == path)
        .ok_or("找不到設備")?;

    let device = device_info.open_device(&api).map_err(|e| e.to_string())?;
    
    let shared_device = Arc::new(Mutex::new(device));
    let is_paused = Arc::new(AtomicBool::new(false));
    let should_stop = Arc::new(AtomicBool::new(false));

    // 儲存狀態
    manager.insert(path.clone(), ManagedDevice {
        device: shared_device.clone(),
        is_paused: is_paused.clone(),
        should_stop: should_stop.clone(),
    });

    // 啟動監聽執行緒
    let app_inner = app.clone();
    let path_inner = path.clone();
    thread::spawn(move || {
        loop {
            if should_stop.load(Ordering::SeqCst) { break; }

            // 如果被暫停（正在發送指令），則稍候再讀取
            if is_paused.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(50));
                continue;
            }

            if let Ok(dev) = shared_device.lock() {
                let mut buf = [0u8; 64];
                // 使用短 timeout 確保能頻繁檢查 pause 狀態
                if let Ok(n) = dev.read_timeout(&mut buf, 100) {
                    if n > 0 {
                        let _ = app_inner.emit("hid-data", buf[..n].to_vec());
                    }
                } else {
                    // 讀取錯誤（可能是拔掉設備）
                    break;
                }
            }
        }
        // 清理狀態
        let state = app_inner.state::<DeviceManager>();
        state.0.lock().unwrap().remove(&path_inner);
    });

    Ok(())
}

#[tauri::command]
async fn send_hid_command(
    path: String, 
    data: Vec<u8>, 
    manager_state: State<'_, DeviceManager>
) -> Result<Vec<u8>, String> {
    // 1. 取得現有的設備句柄，如果不存則自動開啟監聽（可選）
    let (device_arc, pause_flag) = {
        let manager = manager_state.0.lock().unwrap();
        let m_dev = manager.get(&path).ok_or("設備未開啟監聽，請先啟動監聽")?;
        (m_dev.device.clone(), m_dev.is_paused.clone())
    };

    // 2. 暫停監聽執行緒的讀取動作
    pause_flag.store(true, Ordering::SeqCst);

    // 3. 執行寫入與讀取回傳 (使用同一個 Mutex)
    let result = {
        let dev = device_arc.lock().map_err(|_| "鎖定設備失敗")?;
        
        // 格式化數據 (Report ID 0x00 + 64 bytes)
        let mut write_buf = vec![0u8; 65];
        if data[0] == 0x00 {
            let len = std::cmp::min(data.len(), 65);
            write_buf[..len].copy_from_slice(&data[..len]);
        } else {
            let len = std::cmp::min(data.len(), 64);
            write_buf[1..len + 1].copy_from_slice(&data[..len]);
        }

        dev.write(&write_buf).map_err(|e| format!("寫入失敗: {}", e))?;

        // 讀取回覆
        let mut read_buf = [0u8; 64];
        match dev.read_timeout(&mut read_buf, 1000) {
            Ok(n) if n > 0 => Ok(read_buf[..n].to_vec()),
            Ok(_) => Ok(Vec::new()),
            Err(e) => Err(format!("讀取異常: {}", e)),
        }
    };

    // 4. 恢復監聽
    pause_flag.store(false, Ordering::SeqCst);

    result
}

#[tauri::command]
fn stop_listening(path: String, manager_state: State<'_, DeviceManager>) -> Result<(), String> {
    let mut manager = manager_state.0.lock().unwrap();
    if let Some(m_dev) = manager.get(&path) {
        m_dev.should_stop.store(true, Ordering::SeqCst);
    }
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .manage(DeviceManager(Mutex::new(HashMap::new())))
        .invoke_handler(tauri::generate_handler![
            scan_hid_devices, 
            start_listening, 
            stop_listening,
            send_hid_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}