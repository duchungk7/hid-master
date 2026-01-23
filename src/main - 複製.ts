
// RUN: C:\Users\duchu\Desktop\WORKSPACE\Rust_Web\tauri-ws-app>  npm run tauri dev 

import { invoke } from "@tauri-apps/api/core";
//import WebSocket from '@tauri-apps/plugin-websocket';


// 定義與 Rust 後端一致的設備結構
interface HidDevice {
  path: string; // 唯一的設備路徑
  vendor_id: string;
  product_id: string;
  product_string: string | null;
  manufacturer_string: string | null;
  usage_page: number;
}

async function startApp() {
  const app = document.querySelector<HTMLDivElement>('#app');
  if (!app) return;

  // 1. 初始化介面
  app.innerHTML = `
    <div style="font-family: system-ui, sans-serif; padding: 20px; max-width: 800px; margin: 0 auto;">
      <h2>Keystone Mouse HID 控制測試</h2>
      
      <div style="background: #f4f4f4; padding: 15px; border-radius: 8px; margin-bottom: 20px;">
        <button id="scanBtn" style="padding: 8px 15px;">1. 掃描所有裝置</button>
        <div id="status" style="margin-top: 10px; color: #666;">等待操作...</div>
      </div>

      <div style="background: #fffbe6; padding: 15px; border-radius: 8px; border: 1px solid #ffe58f; margin-bottom: 20px;">
        <h3>2. 發送指令給 PowerColor (0x2DBF)</h3>
        <p style="font-size: 12px; color: #856404;">系統會自動根據 VID 尋找對應的設備路徑並補齊至 64 bytes。</p>
        <input id="hexInput" type="text" value="00 C0 0A 00 00" style="width: 300px; padding: 5px;" />
        <button id="sendBtn" style="padding: 5px 15px; background: #52c41a; color: white; border: none; border-radius: 4px; cursor: pointer;">發送指令</button>
      </div>

      <div id="deviceList" style="border: 1px solid #ddd; border-radius: 4px; padding: 10px; background: #fafafa; font-size: 13px; max-height: 300px; overflow-y: auto;">
        裝置清單將顯示於此...
      </div>
    </div>
  `;

  // 取得元素
  const scanBtn = document.getElementById('scanBtn')!;
  const sendBtn = document.getElementById('sendBtn')!;
  const hexInput = document.getElementById('hexInput') as HTMLInputElement;
  const statusDiv = document.getElementById('status')!;
  const deviceListDiv = document.getElementById('deviceList')!;

  // 暫存掃描到的設備清單
  let scannedDevices: HidDevice[] = [];

  // --- 掃描功能 ---
  scanBtn.onclick = async () => {
    statusDiv.innerText = "正在掃描...";
    try {
      scannedDevices = await invoke<HidDevice[]>("scan_hid_devices");
      statusDiv.innerText = `掃描完成，共找到 ${scannedDevices.length} 個裝置`;
      
      deviceListDiv.innerHTML = scannedDevices.map(d => `
        <div style="padding: 5px; border-bottom: 1px solid #eee;">
          <b>${d.product_string || '未知設備'}</b><br/>
          <code style="font-size: 11px; color: #999;">Path: ${d.path}</code><br/>
          <small>VID: ${d.vendor_id} | PID: ${d.product_id} | UsagePage: ${d.usage_page}</small>
        </div>
      `).join('');
    } catch (err) {
      statusDiv.innerText = `掃描失敗: ${err}`;
    }
  };

  // --- 發送功能 ---
  // 在 sendBtn.onclick 邏輯內更新
  sendBtn.onclick = async () => {
    // 1. 精確篩選：VID 0x2DBF | PID 0x5038 | UsagePage 65280
    // 這樣可以避開 UsagePage 1 (KBD)，解決「存取被拒」的問題
    const target = scannedDevices.find(d => 
      d.vendor_id.toLowerCase() === "0x2dbf" && 
      d.product_id.toLowerCase() === "0x5038" &&
      d.usage_page === 65280
    );

    if (!target) {
      statusDiv.innerHTML = `<span style="color: red;">❌ 錯誤：找不到 UsagePage 65280 的控制介面。請先點擊掃描按鈕！</span>`;
      return;
    }

    // 2. 解析 Hex 指令 (例如 "00 C0 0A 00 00")
    const inputStr = hexInput.value.trim();
    if (!inputStr) {
      alert("請輸入 Hex 指令");
      return;
    }
    const dataArray = inputStr.split(/\s+/).map(h => parseInt(h, 16));

    try {
      statusDiv.innerText = `正在通訊中...\n目標路徑：${target.path.substring(0, 40)}...`;

      // 3. 呼叫 Rust 後端 (Rust 會處理 65 bytes 對齊與 3 次重試讀取)
      const resultBytes = await invoke<number[]>("send_hid_command", {
        path: target.path,
        data: dataArray
      });

      // 4. 模擬 C++ 格式化打印結果
      if (resultBytes && resultBytes.length > 0) {
        let debugOutput = `=============== return: ${resultBytes.length} bytes ===============\n`;
        
        // 每 16 bytes 換一行顯示
        for (let i = 0; i < resultBytes.length; i += 16) {
          const chunk = resultBytes.slice(i, i + 16);
          const hexLine = chunk
            .map(b => b.toString(16).toUpperCase().padStart(2, '0'))
            .join(' ');
          debugOutput += hexLine + "\n";
        }

        statusDiv.innerHTML = `
          <span style="color: green; font-weight: bold;">✅ 指令發送成功並收到回應：</span>
          <pre style="background: #1e1e1e; color: #00ff00; padding: 12px; border-radius: 6px; font-family: 'Consolas', monospace; margin-top: 10px; line-height: 1.5; border: 1px solid #333;">${debugOutput}</pre>
        `;
      } else {
        statusDiv.innerHTML = `
          <span style="color: orange; font-weight: bold;">⚠️ 寫入成功，但設備未回傳資料。</span><br/>
          <small>這可能代表指令不正確，或設備不需要針對此指令回傳確認。 (已重試 3 次)</small>
        `;
      }
    } catch (err) {
      console.error("HID 錯誤細節:", err);
      statusDiv.innerHTML = `<span style="color: red; font-weight: bold;">❌ 失敗：${err}</span>`;
    }
  };
}

window.addEventListener('DOMContentLoaded', startApp);