
// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]


use hidapi::HidApi;
use serde::Serialize;


/*
這段程式碼是 Rust 中定義 資料結構（Struct） 的典型寫法，結合了 Tauri 跨平台開發時常用的 序列化（Serialization） 功能。
#[derive(Serialize)]
語法名稱：屬性（Attribute）/ 衍生宏（Derive Macro）。

詳細說明：
    #[] 是 Rust 的屬性語法，用來給編譯器額外的指示。
    derive 告訴 Rust 自動為這個結構體實作特定的 Trait（特徵）。
    Serialize 來自 serde 庫。它的作用是讓這個結構體可以被轉換成其他格式（例如 JSON）。
 */
#[derive(Serialize)]

// 定義回傳給前端的設備資訊結構
struct HidDevice {
    path: String, // 設備的唯一路徑
    vendor_id: String,
    product_id: String,
    product_string: Option<String>,  // Option 是 Rust 極其重要的安全性語法。它代表這個欄位 「可能存在，也可能不存在（為空）」。
    usage_page: u16, // 用來判斷設備用途  無符號 16 位元整數（Unsigned 16-bit Integer）u16 代表範圍從 0 到 2^16 -1 (65535) 的正整數。
}



//這行告訴 Tauri 框架：「這個函式是一個可以從前端 JavaScript 透過 invoke() 呼叫的指令」。沒有這行，前端就找不到這個後端函式。
#[tauri::command]
/*
fn：定義函式的關鍵字。
Result<Ok型別, Err型別>：Rust 的錯誤處理機制。
    Ok(Vec<HidDevice>)：成功時，傳回一個裝滿 HidDevice 結構體的向量（陣列）。
    Err(String)：失敗時，傳回一個錯誤訊息字串。
 */
fn scan_hid_devices() -> Result<Vec<HidDevice>, String> {
    // HidApi::new()：嘗試初始化 HIDAPI 庫。
    // .map_err(|e| e.to_string())：如果初始化失敗，將內部的錯誤物件轉化為可讀的 String。
    //? (問號運算子)：這是 Rust 的語法。如果前面失敗了，直接 return 錯誤給呼叫者；如果成功，則解開包裝將 api 實例賦值給變數。
    let api = HidApi::new().map_err(|e| e.to_string())?;
    // HidApi 更多說明可以查看 https://docs.rs/hidapi/latest/hidapi/struct.HidApi.html
    Ok(api.device_list()
        .map(|device| HidDevice {
            path: device.path().to_string_lossy().to_string(),
            vendor_id: format!("{:#06x}", device.vendor_id()),
            product_id: format!("{:#06x}", device.product_id()),
            product_string: device.product_string().map(|s| s.to_string()),
            usage_page: device.usage_page(),
        })
        .collect())
}

#[tauri::command]
async fn send_hid_command(path: String, data: Vec<u8>) -> Result<Vec<u8>, String> {
    let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
    
    // 1. 開啟設備
    let device = api.open_path(&std::ffi::CString::new(path).unwrap())
        .map_err(|e| format!("開啟失敗: {}", e))?;

    // 2. 準備 65 位元組 Report (1 byte ID + 64 bytes Data)
    let mut report = vec![0u8; 65];
    for (i, &byte) in data.iter().enumerate() {
        if i < report.len() { report[i] = byte; }
    }

    // --- [Debug] 打印發送內容，格式模擬 C++ ---
    println!("\n=== Sending HID Report (65 bytes) ===");
    for (i, byte) in report.iter().enumerate() {
        print!("{:02X} ", byte);
        if (i + 1) % 16 == 0 { println!(); }
    }
    println!("\n=====================================");

    // 3. 執行 Write 與 Read (包含 Retry 邏輯)
    device.write(&report).map_err(|e| format!("寫入失敗: {}", e))?;

    let mut read_buf = [0u8; 64];
    for _ in 0..3 {
        match device.read_timeout(&mut read_buf, 100) {
            Ok(res) if res > 0 => return Ok(read_buf[..res].to_vec()),
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }

    Ok(Vec::new())
}



// 記得在 main 函數中註冊這個新指令
fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            scan_hid_devices, 
            send_hid_command // 註冊發送指令
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}


