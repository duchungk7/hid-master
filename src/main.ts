
// RUN: C:\Users\duchu\Desktop\WORKSPACE\Rust_Web\tauri-ws-app>  npm run tauri dev 

import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from '@tauri-apps/api/event';

// --- 1. 型別定義 (新增 interface_number) ---
interface HidDevice {
  path: string;
  vendor_id: string;
  product_id: string;
  product_string: string | null;
  manufacturer_string: string | null;
  usage_page: number;
  interface_number: number; // 跨平台區分介面的關鍵
}

let scannedDevices: HidDevice[] = [];
let selectedDevicePath: string | null = null;
let unlistenHid: UnlistenFn | null = null;
const activeListeners = new Set<string>();

// --- 2. 跨平台相容的時間日誌 ---
function addLog(message: string, type: 'info' | 'incoming' | 'outgoing' | 'error' = 'info') {
  const logDiv = document.querySelector('#log') as HTMLElement;
  if (!logDiv) return;

  const d = new Date();
  const ms = d.getMilliseconds().toString().padStart(3, '0');
  const time = `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}.${ms}`;

  const colors = { 
    info: '#888', 
    incoming: '#00ff00', 
    outgoing: '#00bfff', 
    error: '#ff4d4f' 
  };

  const entry = document.createElement('div');
  entry.style.color = colors[type];
  entry.style.marginBottom = '2px';
  entry.innerHTML = `<span style="color: #444;">[${time}]</span> ${message}`;
  
  logDiv.appendChild(entry);
  logDiv.scrollTop = logDiv.scrollHeight;
}

// --- 3. 初始化監聽 (Async Event) ---
async function initEventListener() {
  if (unlistenHid) unlistenHid();
  unlistenHid = await listen<number[]>("hid-data", (event) => {
    const hex = event.payload.map(b => b.toString(16).toUpperCase().padStart(2, '0')).join(' ');
    addLog(`[ASYNC IN] ${hex}`, 'incoming');
  });
}

// --- 4. UI 與主邏輯 ---
async function startApp() {
  const app = document.querySelector<HTMLDivElement>('#app');
  if (!app) return;

  app.innerHTML = `
    <div style="background: #121212; color: #eee; font-family: 'Segoe UI', Tahoma, sans-serif; height: 100vh; display: flex; flex-direction: column; padding: 15px; box-sizing: border-box;">
      <header style="border-bottom: 1px solid #333; padding-bottom: 10px; margin-bottom: 15px; display: flex; justify-content: space-between; align-items: center;">
        <div>
           <span style="color: #52c41a; font-weight: bold; font-size: 1.1rem;">KEYSTONE CROSS-PLATFORM HID</span>
           <div style="font-size: 10px; color: #555;">Support: Windows / Linux / macOS</div>
        </div>
        <div id="selectedTag" style="font-size: 12px; background: #222; padding: 4px 12px; border-radius: 4px; color: #888;">Wait for Device...</div>
      </header>

      <div style="display: flex; flex: 1; gap: 15px; overflow: hidden;">
        <div style="width: 360px; display: flex; flex-direction: column; gap: 15px;">
          <button id="scanBtn" style="width: 100%; padding: 10px; cursor: pointer; background: #333; color: white; border: 1px solid #444; border-radius: 4px;">1. Scan Devices</button>
          
          <div style="flex: 1; background: #000; border: 1px solid #333; border-radius: 4px; overflow: hidden; display: flex; flex-direction: column;">
            <div style="background: #222; padding: 5px 10px; font-size: 11px; color: #888;">HID Device List</div>
            <div id="deviceList" style="flex: 1; overflow-y: auto; padding: 5px;"></div>
          </div>

          <div style="background: #1e1e1e; padding: 15px; border-radius: 4px; border: 1px solid #333;">
            <label style="font-size: 11px; color: #aaa; display: block; margin-bottom: 5px;">HEX COMMAND (Space separated):</label>
            <input id="hexInput" type="text" value="00 C0 0A 00 00" style="width: 100%; background: #000; color: #00ff00; border: 1px solid #444; padding: 10px; box-sizing: border-box; outline: none; margin-bottom: 10px; font-family: monospace;" />
            <button id="sendBtn" style="width: 100%; padding: 12px; background: #52c41a; color: #000; font-weight: bold; border: none; cursor: pointer; border-radius: 4px;">SEND TO HARDWARE</button>
          </div>
        </div>

        <div style="flex: 1; display: flex; flex-direction: column; border: 1px solid #333; background: #000; border-radius: 4px;">
          <div style="background: #222; padding: 5px 12px; font-size: 11px; display: flex; justify-content: space-between; align-items: center;">
            <span>DATA MONITOR (HEX)</span>
            <span id="clearLog" style="cursor: pointer; color: #666; font-size: 10px;">[CLEAR]</span>
          </div>
          <div id="log" style="flex: 1; padding: 12px; overflow-y: auto; font-size: 12px; line-height: 1.4; font-family: 'Consolas', 'Monaco', monospace;"></div>
        </div>
      </div>
    </div>
  `;

  const scanBtn = document.getElementById('scanBtn') as HTMLButtonElement;
  const sendBtn = document.getElementById('sendBtn') as HTMLButtonElement;
  const hexInput = document.getElementById('hexInput') as HTMLInputElement;
  const deviceList = document.getElementById('deviceList') as HTMLElement;
  const selectedTag = document.getElementById('selectedTag') as HTMLElement;
  const clearLog = document.getElementById('clearLog') as HTMLElement;

  await initEventListener();
  clearLog.onclick = () => { document.getElementById('log')!.innerHTML = ''; };

  // --- 列表渲染 (優化跨平台顯示) ---
  function renderDeviceList() {
    deviceList.innerHTML = scannedDevices.map((d) => {
      const isSelected = d.path === selectedDevicePath;
      const isPowerColor = d.vendor_id.toLowerCase().includes("2dbf");
      const isControlUP = d.usage_page === 65280;

      return `
        <div class="device-item" data-path="${d.path}" 
             style="padding: 12px; margin-bottom: 6px; cursor: pointer; border-radius: 4px; border: 1px solid ${isSelected ? '#52c41a' : '#333'}; background: ${isSelected ? '#1b2a1b' : '#111'};">
          <div style="font-weight: bold; color: ${isControlUP ? '#52c41a' : '#ddd'};">
            ${d.product_string || 'Generic HID Device'} 
            ${isPowerColor ? '<span style="color:#52c41a; font-size:9px; margin-left:5px;">(Target)</span>' : ''}
          </div>
          <div style="font-size: 10px; color: #666; margin-top: 5px; font-family: monospace;">
            UP: ${d.usage_page} | VID: ${d.vendor_id} | PID: ${d.product_id} <br/>
            IF: ${d.interface_number} | Path: ${d.path.substring(0, 25)}...
          </div>
        </div>
      `;
    }).join('');

    document.querySelectorAll('.device-item').forEach(item => {
      item.addEventListener('click', () => {
        selectedDevicePath = (item as HTMLElement).dataset.path || null;
        const device = scannedDevices.find(d => d.path === selectedDevicePath);
        if (device) {
          selectedTag.innerText = `Active: ${device.product_string} (IF:${device.interface_number})`;
          selectedTag.style.color = "#52c41a";
          addLog(`Device selected. Interface: ${device.interface_number}`, 'info');
        }
        renderDeviceList();
      });
    });
  }

  // --- 掃描功能 ---
  scanBtn.onclick = async () => {
    try {
      addLog("Scanning HID bus...", 'info');
      scannedDevices = await invoke<HidDevice[]>("scan_hid_devices");
      renderDeviceList();
      addLog(`Found ${scannedDevices.length} devices.`, 'info');
    } catch (e) {
      addLog(`Scan Error: ${e}`, 'error');
    }
  };

  // --- 發送功能 ---
  sendBtn.onclick = async () => {
    if (!selectedDevicePath) {
      addLog("Error: No device selected.", 'error');
      return;
    }

    const hexCmd = hexInput.value.trim();
    if (!hexCmd) return;

    const cmdArray = hexCmd.split(/\s+/).map(h => parseInt(h, 16));
    addLog(`[OUT] ${hexCmd}`, 'outgoing');

    try {
      // 1. 啟動背景監聽 (如果尚未啟動)
      if (!activeListeners.has(selectedDevicePath)) {
        addLog(`[SYS] Starting background worker for current OS...`, 'info');
        await invoke("start_listening", { path: selectedDevicePath });
        activeListeners.add(selectedDevicePath);
      }

      // 2. 下指令
      const response = await invoke<number[]>("send_hid_command", {
        path: selectedDevicePath,
        data: cmdArray
      });

      // 3. 顯示結果 (即使有背景監聽，我們通常還是會顯示一次直接結果)
      if (response && response.length > 0) {
        const respHex = response.map(b => b.toString(16).toUpperCase().padStart(2, '0')).join(' ');
        addLog(`[RESULT] ${respHex}`, 'info');
      }

    } catch (e) {
      addLog(`Communication Failure: ${e}`, 'error');
      // 如果出錯，清除監聽標記以便重試
      activeListeners.delete(selectedDevicePath);
    }
  };
}

window.addEventListener('DOMContentLoaded', startApp);