import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import { Terminal, Crosshair, Activity, Shield, Settings, Power, Trash2, Plus, Save, Bell, XCircle } from 'lucide-react';
import clsx from 'clsx';

// --- Types ---

interface OCRResult {
  text: string;
  box: number[][];
  confidence: number;
}

interface Rule {
  id: string;
  trigger_text: string[];
  max_value?: number;
  min_value?: number;
  target_attribute?: string;
  cooldown: number;
}

interface AppConfig {
  target_window: string;
  global_action_key: string;
  hold_duration: number;
  rules: Rule[];
  discord_webhook_url?: string;
  notify_on_success?: boolean;
  notify_on_failure?: boolean;
  notify_on_error?: boolean;
  agreed_to_terms?: boolean;
  gas_url?: string;
  api_secret?: string;
}

interface LogEntry {
  timestamp: string;
  log_type: "System" | "Ocr" | "Logic" | "Action";
  message: string;
}

const RARITY_TIERS = [
  { label: "Common", color: "text-zinc-400", bg: "bg-zinc-800" },
  { label: "Rare", color: "text-blue-400", bg: "bg-blue-900" },
  { label: "Epic", color: "text-purple-400", bg: "bg-purple-900" },
  { label: "Legendary", color: "text-amber-400", bg: "bg-amber-900" },
  { label: "Mythic", color: "text-red-500", bg: "bg-red-900" },
  { label: "Brainrot God", color: "text-rose-500", bg: "bg-rose-950" },
  { label: "Secret", color: "text-pink-500", bg: "bg-pink-950" },
];

export default function App() {
  const [status, setStatus] = useState("SYSTEM_OFFLINE");
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [previewImage, setPreviewImage] = useState<string | null>(null);
  const [ocrResults, setOcrResults] = useState<OCRResult[]>([]);
  const [windows, setWindows] = useState<string[]>([]);
  const [showSettings, setShowSettings] = useState(false);

  // Config State
  const [config, setConfig] = useState<AppConfig>({
    target_window: "",
    global_action_key: "e",
    hold_duration: 1.2,
    rules: [],
    discord_webhook_url: "",
    notify_on_success: true,
    notify_on_failure: false,
    notify_on_error: true,
    gas_url: "",
    api_secret: ""
  });

  // Wizard State
  const [wizardStep, setWizardStep] = useState(1);
  const [wizKeyword, setWizKeyword] = useState("");
  const [wizRarity, setWizRarity] = useState("");
  const [wizMaxPrice, setWizMaxPrice] = useState("");
  const [wizMinProfit, setWizMinProfit] = useState("");
  const [wizAttribute, setWizAttribute] = useState("");

  // Liability State
  const [showLiability, setShowLiability] = useState(false);

  // License State
  const [isLocked, setIsLocked] = useState(true);
  const [machineId, setMachineId] = useState("LOADING...");
  const [activationKey, setActivationKey] = useState("");
  const [adminKeys, setAdminKeys] = useState<{ priv: string, pub: string } | null>(null);

  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const unlistenStatus = listen('bot-event', (event: any) => {
      const payload = event.payload;
      if (payload.type === 'status') {
        setStatus(payload.data.message);
      }
    });

    const unlistenLogs = listen('log-message', (event: any) => {
      setLogs(prev => [event.payload, ...prev].slice(0, 50));
    });

    const unlistenFrame = listen('frame-update', (event: any) => {
      setPreviewImage(event.payload);
    });

    const unlistenOcr = listen('ocr-data', (event: any) => {
      // event.payload is Vec<OcrData> { text, x, y, w, h }
      // These coords are relative to ROI (320, 180).
      // We need to adjust them for the Full HD stream (1920x1080)
      const rawResults = event.payload as any[];
      const adjusted = rawResults.map(r => ({
        ...r,
        // box: r.x, y, w, h are floats or ints
        x: r.x + 320,
        y: r.y + 0
      }));
      setOcrResults(adjusted);
    });

    // License Check
    const checkLicense = async () => {
      const mid = await invoke<string>('get_machine_id');
      setMachineId(mid);

      const storedKey = localStorage.getItem('antigravity_license_key');
      if (storedKey) {
        const isValid = await invoke<boolean>('verify_activation_key', { key: storedKey });
        if (isValid) {
          setIsLocked(false);
          setActivationKey(storedKey);
          return;
        }
      }
    };

    checkLicense();

    refreshWindows();
    loadConfig();

    // Check Liability
    const checkLiability = async () => {
      const cfg: any = await invoke('get_config');
      if (cfg && !cfg.agreed_to_terms) {
        setShowLiability(true);
      }
    };
    checkLiability();

    // Auto-Update Check
    const checkUpdate = async () => {
      try {
        const { check } = await import('@tauri-apps/plugin-updater');
        const { relaunch } = await import('@tauri-apps/plugin-process');

        const update = await check();
        if (update?.available) {
          const yes = confirm(`Update to v${update.version} is available.\n\nRelease Notes:\n${update.body}\n\nInstall now?`);
          if (yes) {
            await update.downloadAndInstall();
            await relaunch();
          }
        }
      } catch (e) {
        console.error("Update check failed:", e);
      }
    };
    checkUpdate();

    return () => {
      unlistenStatus.then(f => f());
      unlistenLogs.then(f => f());
      unlistenFrame.then(f => f());
      unlistenOcr.then(f => f());
    };
  }, []);

  // Canvas Drawing Effect
  useEffect(() => {
    const canvas = canvasRef.current;
    const container = canvas?.parentElement;
    if (!canvas || !container) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Clear
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // Coordinate System:
    // Stream is 1920x1080.
    canvas.width = 1920;
    canvas.height = 1080;

    // Draw Boxes
    ocrResults.forEach((res: any) => {
      // Bounding Box
      ctx.strokeStyle = "rgba(0, 255, 0, 0.8)";
      ctx.lineWidth = 4;
      ctx.strokeRect(res.x, res.y, res.w, res.h);

      // Text Label Shadow
      ctx.font = "bold 32px monospace";
      ctx.fillStyle = "rgba(0, 0, 0, 0.8)";
      ctx.fillText(res.text, res.x + 2, res.y - 8 + 2);

      // Text Label
      ctx.fillStyle = "rgba(0, 255, 0, 1.0)";
      ctx.fillText(res.text, res.x, res.y - 8);
    });

    // Draw ROI Border (Visual Guide)
    ctx.strokeStyle = "rgba(255, 0, 0, 0.5)";
    ctx.lineWidth = 6;
    ctx.setLineDash([10, 10]);
    ctx.strokeRect(320, 0, 1280, 1080);
    ctx.setLineDash([]);

  }, [ocrResults]);

  // --- API ---

  const refreshWindows = async () => {
    try {
      setWindows(await invoke('list_windows'));
    } catch (e) { console.error(e); }
  };

  const loadConfig = async () => {
    try {
      const cfg: any = await invoke('get_config');
      if (cfg) {
        setConfig({
          target_window: cfg.target_window || "",
          global_action_key: cfg.global_action_key || "e",
          hold_duration: cfg.hold_duration || 1.2,
          rules: cfg.rules || [],
          discord_webhook_url: cfg.discord_webhook_url || "",
          notify_on_success: cfg.notify_on_success ?? true,
          notify_on_failure: cfg.notify_on_failure ?? false,
          notify_on_error: cfg.notify_on_error ?? true
        });
      }
    } catch (e) { console.error(e); }
  };

  const saveConfig = async (newCfg = config) => {
    try {
      await invoke('update_config', { newConfig: newCfg });
      setLogs(p => [{ timestamp: new Date().toLocaleTimeString(), log_type: "System", message: "設定を保存しました" }, ...p]);
    } catch (e) { console.error(e); }
  };

  const toggleEngine = async () => {
    await saveConfig();
    try {
      await invoke('start_rust_engine');
    } catch (e) { console.error(e); }
  };

  const stopEngine = async () => {
    try { await invoke('stop_rust_engine'); } catch (e) { console.error(e); }
  };

  const activateLicense = async () => {
    try {
      const isValid = await invoke<boolean>('verify_activation_key', { key: activationKey });
      if (isValid) {
        localStorage.setItem('antigravity_license_key', activationKey);
        setIsLocked(false);
        setLogs(p => [{ timestamp: new Date().toLocaleTimeString(), log_type: "System", message: "ACCESS GRANTED." }, ...p]);
      } else {
        alert("ACCESS DENIED. Invalid Key.");
      }
    } catch (e) { console.error(e); }
  };

  const adminGen = async () => {
    const keys = await invoke<[string, string]>('generate_admin_keys');
    setAdminKeys({ priv: keys[0], pub: keys[1] });
  };

  // --- Logic ---

  const addRule = () => {
    if (!wizMaxPrice && !wizMinProfit && !wizKeyword && !wizRarity) return;

    // Build triggers
    const triggers: string[] = []; // Typed
    if (wizKeyword) triggers.push(wizKeyword);
    if (wizRarity) {
      const idx = RARITY_TIERS.findIndex(r => r.label === wizRarity);
      if (idx !== -1) {
        RARITY_TIERS.slice(idx).forEach(r => {
          if (!triggers.includes(r.label)) triggers.push(r.label);
        });
      }
    }
    if (triggers.length === 0) triggers.push("ANY_ITEM");

    const newRule: Rule = {
      id: Math.random().toString(36).substr(2, 9),
      trigger_text: triggers,
      max_value: wizMaxPrice ? parseFloat(wizMaxPrice) : undefined,
      min_value: wizMinProfit ? parseFloat(wizMinProfit) : undefined,
      target_attribute: wizAttribute || undefined,
      cooldown: 2.0
    };

    const newRules = [...config.rules, newRule];
    setConfig({ ...config, rules: newRules });
    saveConfig({ ...config, rules: newRules });

    // Reset wizard
    setWizardStep(1);
    setWizKeyword("");
    setWizRarity("");
    setWizMaxPrice("");
    setWizMinProfit("");
    setWizAttribute("");
  };

  const removeRule = (id: string) => {
    const newRules = config.rules.filter(r => r.id !== id);
    setConfig({ ...config, rules: newRules });
    saveConfig({ ...config, rules: newRules });
  };

  const sendTestWebhook = () => {
    // Mock test
    setLogs(p => [{ timestamp: new Date().toLocaleTimeString(), log_type: "System", message: "Discordテスト通知を送信しました" }, ...p]);
  };

  // --- Render ---

  return (
    <div className="min-h-screen bg-black text-green-500 font-mono overflow-hidden select-none flex flex-col relative">

      {/* LOCK SCREEN */}
      {isLocked && (
        <div className="absolute inset-0 z-[100] bg-black flex items-center justify-center p-10 flex-col">
          <div className="max-w-xl w-full border border-green-900 bg-zinc-950 p-8 shadow-[0_0_50px_rgba(0,255,0,0.1)]">
            <h1 className="text-2xl font-bold tracking-[0.2em] text-white mb-2 text-center">ANTIGRAVITY <span className="text-green-600">LOCKED</span></h1>
            <p className="text-xs text-zinc-500 text-center mb-8">SECURE ACCESS REQUIRED. AUTHORIZATION PROTOCOL ACTIVE.</p>

            <div className="mb-6 space-y-2">
              <label className="text-xs font-bold text-zinc-400">CHALLENGE (MACHINE ID):</label>
              <div className="flex gap-2">
                <code className="flex-1 bg-black border border-zinc-800 p-2 text-green-500 font-mono text-sm tracking-wide">{machineId}</code>
                <button onClick={() => navigator.clipboard.writeText(machineId)} className="bg-zinc-900 border border-zinc-700 px-3 hover:bg-zinc-800 text-xs">COPY</button>
              </div>
            </div>

            <div className="mb-8 space-y-2">
              <label className="text-xs font-bold text-zinc-400">RESPONSE (LICENSE KEY):</label>
              <textarea
                value={activationKey}
                onChange={e => setActivationKey(e.target.value)}
                placeholder="PASTE ENCRYPTED SIGNATURE HERE..."
                className="w-full h-24 bg-black border border-zinc-800 p-2 text-white font-mono text-xs focus:border-green-500 outline-none resize-none"
              />
            </div>

            <button onClick={activateLicense} className="w-full bg-green-900/30 border border-green-600 text-green-400 font-bold py-3 hover:bg-green-800/50 transition tracking-widest">
              INITIALIZE CONNECTION
            </button>

            <div className="mt-8 pt-4 border-t border-zinc-900 text-center">
              <button onClick={adminGen} className="text-[10px] text-zinc-700 hover:text-zinc-500">[ADMIN TOOL: KEYGEN]</button>
              {adminKeys && (
                <div className="mt-2 text-left bg-zinc-900 p-2 text-[10px] space-y-2 overflow-hidden">
                  <div>
                    <span className="text-red-500 font-bold">PRIVATE (KEEP SAFE):</span>
                    <div className="break-all select-all text-zinc-400">{adminKeys.priv}</div>
                  </div>
                  <div>
                    <span className="text-blue-500 font-bold">PUBLIC (PUT IN APP):</span>
                    <div className="break-all select-all text-zinc-400">{adminKeys.pub}</div>
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* HEADER */}
      <header className="h-12 border-b border-green-900 bg-zinc-950 flex items-center px-4 justify-between shrink-0">
        <div className="flex items-center gap-2">
          <Terminal className="w-5 h-5 text-green-400" />
          <span className="text-lg font-bold tracking-widest text-white">PYAUTO_OPERATOR <span className="text-green-600 text-xs">V4.0 PRO</span></span>
        </div>
        <div className="flex items-center gap-4 text-xs">
          <div className={clsx("px-3 py-1 border flex items-center gap-2", status.includes("Started") ? "border-green-500 bg-green-900/30 text-green-400" : "border-red-900 bg-zinc-900 text-zinc-500")}>
            <Activity className={clsx("w-3 h-3", status.includes("Started") && "animate-pulse")} />
            {status.includes("Started") ? "SYSTEM ONLINE" : "SYSTEM STANDBY"}
          </div>
          <button onClick={() => setShowSettings(true)} className="p-2 hover:bg-zinc-800 rounded text-zinc-400 hover:text-white transition">
            <Settings className="w-5 h-5" />
          </button>
        </div>
      </header>

      {/* MODAL: SETTINGS */}
      {showSettings && (
        <div className="absolute inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-8">
          <div className="bg-zinc-950 border border-green-800 w-full max-w-2xl shadow-2xl shadow-green-900/20 flex flex-col">
            <div className="p-4 border-b border-green-900 bg-zinc-900 flex justify-between items-center">
              <h2 className="text-lg font-bold flex items-center gap-2"><Settings className="w-5 h-5" /> システム設定 (System Config)</h2>
              <button onClick={() => setShowSettings(false)}><XCircle className="w-6 h-6 text-zinc-500 hover:text-red-500" /></button>
            </div>
            <div className="p-6 space-y-6">

              {/* DISCORD */}
              <div className="space-y-2">
                <label className="text-sm font-bold text-zinc-300 flex items-center gap-2">
                  <Bell className="w-4 h-4 text-[#5865F2]" /> Discord Uplink
                </label>
                <input
                  type="password"
                  value={config.discord_webhook_url}
                  onChange={e => setConfig({ ...config, discord_webhook_url: e.target.value })}
                  placeholder="https://discord.com/api/webhooks/..."
                  className="w-full bg-black border border-zinc-700 p-2 text-sm focus:border-[#5865F2] outline-none text-white"
                />
                <p className="text-xs text-zinc-500">チャンネル設定からWebhook URLを取得して貼り付けてください。</p>

                <div className="flex gap-4 mt-2">
                  <label className="flex items-center gap-2 text-xs cursor-pointer">
                    <input type="checkbox" checked={config.notify_on_success} onChange={e => setConfig({ ...config, notify_on_success: e.target.checked })} />
                    購入成功時通知 (SSR Get!)
                  </label>
                  <label className="flex items-center gap-2 text-xs cursor-pointer">
                    <input type="checkbox" checked={config.notify_on_error} onChange={e => setConfig({ ...config, notify_on_error: e.target.checked })} />
                    エラー/停止時のみ
                  </label>
                </div>
                <button onClick={sendTestWebhook} className="text-xs border border-zinc-700 px-3 py-1 hover:bg-zinc-800 mt-2">TEST UPLINK</button>
              </div>

              <div className="border-t border-zinc-800 pt-4 grid grid-cols-2 gap-4">
                <div>
                  <label className="text-xs font-bold text-zinc-400">GLOBAL TRIGGER KEY</label>
                  <input
                    value={config.global_action_key}
                    onChange={e => setConfig({ ...config, global_action_key: e.target.value })}
                    className="w-full bg-black border border-zinc-700 p-1 text-center font-bold text-green-400 mt-1"
                  />
                </div>
                <div>
                  <label className="text-xs font-bold text-zinc-400">HOLD DURATION (sec)</label>
                  <input
                    type="number" step="0.1"
                    value={config.hold_duration}
                    onChange={e => setConfig({ ...config, hold_duration: parseFloat(e.target.value) })}
                    className="w-full bg-black border border-zinc-700 p-1 text-center font-bold text-green-400 mt-1"
                  />
                </div>
                {/* GAS Uplink Config */}
                <div className="col-span-2 border-t border-zinc-800 my-2 pt-2">
                  <label className="text-xs font-bold text-zinc-500 mb-2 block">GOOGLE APPS SCRIPT CONFIGURATION</label>
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <label className="text-xs font-bold text-zinc-400">GAS WEB APP URL</label>
                      <input
                        value={config.gas_url || ""}
                        onChange={e => setConfig({ ...config, gas_url: e.target.value })}
                        placeholder="https://script.google.com/..."
                        className="w-full bg-black border border-zinc-700 p-1 font-mono text-xs text-green-400 mt-1"
                      />
                    </div>
                    <div>
                      <label className="text-xs font-bold text-zinc-400">API SECRET TOKEN</label>
                      <input
                        value={config.api_secret || ""}
                        onChange={e => setConfig({ ...config, api_secret: e.target.value })}
                        type="password"
                        placeholder="Secret Token"
                        className="w-full bg-black border border-zinc-700 p-1 font-mono text-xs text-green-400 mt-1"
                      />
                    </div>
                  </div>
                </div>
              </div>
            </div>
            <div className="p-4 border-t border-green-900 bg-zinc-900 flex justify-end">
              <button
                onClick={() => { saveConfig(); setShowSettings(false); }}
                className="bg-green-700 text-white px-6 py-2 hover:bg-green-600 font-bold text-sm tracking-wide"
              >
                SAVE & CLOSE
              </button>
            </div>
          </div>
        </div>
      )}

      {/* MAIN LAYOUT */}
      <div className="flex-1 flex overflow-hidden">

        {/* LEFT: VISUAL */}
        <div className="w-2/3 flex flex-col border-r border-green-900 relative bg-zinc-950">
          {/* TOOLBAR */}
          <div className="p-2 border-b border-green-900 flex gap-2 items-center justify-between bg-zinc-900/50">
            <div className="flex items-center gap-2">
              <Crosshair className="w-4 h-4 text-zinc-500" />
              <select
                value={config.target_window}
                onChange={e => setConfig({ ...config, target_window: e.target.value })}
                className="bg-black border border-zinc-700 text-xs p-1 w-48 focus:border-green-500"
              >
                <option value="">-- TARGET WINDOW --</option>
                {windows.map(w => <option key={w} value={w}>{w}</option>)}
              </select>
              <button onClick={refreshWindows} className="text-xs text-zinc-500 hover:text-white">[再スキャン]</button>
            </div>

            <button
              onClick={status.includes("Started") ? stopEngine : toggleEngine}
              className={clsx(
                "flex items-center gap-3 px-8 py-1.5 font-bold text-sm border transition-all tracking-wider shadow-lg",
                status.includes("Started")
                  ? "border-red-500 text-red-500 bg-red-950/30 hover:bg-red-900/50"
                  : "border-green-500 text-green-500 bg-green-950/30 hover:bg-green-900/50"
              )}
            >
              <Power className="w-4 h-4" />
              {status.includes("Started") ? "ABORT SEQUENCE" : "システム起動 (INITIATE)"}
            </button>

            {/* MANUAL UPLOAD */}
            <label className="flex items-center gap-2 px-3 py-1.5 border border-blue-500 bg-blue-900/20 text-blue-400 text-xs font-bold hover:bg-blue-900/40 cursor-pointer">
              <input type="file" className="hidden" accept="image/png, image/jpeg" onChange={async (e) => {
                const file = e.target.files?.[0];
                if (!file) return;

                // Reset val
                e.target.value = "";

                try {
                  setLogs(p => [{ timestamp: new Date().toLocaleTimeString(), log_type: "System", message: "Uploading..." }, ...p]);
                  const arrayBuffer = await file.arrayBuffer();
                  const bytes = Array.from(new Uint8Array(arrayBuffer));

                  const res = await invoke<string>('manual_ingest', {
                    fileName: file.name,
                    fileData: bytes
                  });

                  alert(res);
                  setLogs(p => [{ timestamp: new Date().toLocaleTimeString(), log_type: "Action", message: "Manual Upload Complete" }, ...p]);
                } catch (err) {
                  alert("Upload Error: " + err);
                  console.error(err);
                }
              }} />
              <span>UPLOAD EVIDENCE</span>
            </label>
          </div>

          {/* CANVAS */}
          <div className="flex-1 relative flex items-center justify-center overflow-hidden">
            {previewImage ? (
              <div className="relative w-full h-full">
                <img src={`data:image/jpeg;base64,${previewImage}`} className="w-full h-full object-contain opacity-70" />
                <canvas ref={canvasRef} className="absolute inset-0 w-full h-full object-contain pointer-events-none" />
                <div className="absolute inset-0 pointer-events-none bg-[url('https://transparenttextures.com/patterns/carbon-fibre.png')] opacity-10" />
                {/* Scanlines */}
                <div className="absolute inset-0 bg-[linear-gradient(rgba(18,16,16,0)_50%,rgba(0,0,0,0.25)_50%),linear-gradient(90deg,rgba(255,0,0,0.06),rgba(0,255,0,0.02),rgba(0,0,255,0.06))] bg-[length:100%_4px,6px_100%] pointer-events-none opacity-50" />
              </div>
            ) : (
              <div className="text-zinc-600 flex flex-col items-center">
                <Shield className="w-24 h-24 mb-4 opacity-10" />
                <span className="text-sm tracking-widest">AWAITING SIGNAL...</span>
              </div>
            )}
          </div>
        </div>

        {/* RIGHT: CONTROL CENTER */}
        <div className="w-1/3 flex flex-col bg-zinc-900">

          {/* RULE WIZARD */}
          <div className="p-4 border-b border-green-900 bg-zinc-950">
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-xs font-bold text-green-400 flex items-center gap-2"><Plus className="w-3 h-3" /> CREATE NEW BUY RULE</h3>
              <div className="flex gap-1">
                <span className={clsx("w-2 h-2 rounded-full", wizardStep === 1 ? "bg-green-500" : "bg-zinc-700")} />
                <span className={clsx("w-2 h-2 rounded-full", wizardStep === 2 ? "bg-green-500" : "bg-zinc-700")} />
              </div>
            </div>

            {wizardStep === 1 ? (
              <div className="space-y-3 animate-in fade-in slide-in-from-right-4">
                <div>
                  <label className="text-[10px] text-zinc-500 block mb-1">キーワード (名前・属性)</label>
                  <input
                    value={wizKeyword}
                    onChange={e => setWizKeyword(e.target.value)}
                    placeholder="例: Dragon, Fire (空欄で全対象)"
                    className="w-full bg-black border border-zinc-700 text-xs p-2 text-white placeholder-zinc-600 focus:border-green-500 outline-none"
                  />
                </div>
                <div>
                  <label className="text-[10px] text-zinc-500 block mb-1">属性 (任意) (ATTRIBUTE)</label>
                  <input
                    value={wizAttribute}
                    onChange={e => setWizAttribute(e.target.value)}
                    placeholder="例: Fire (一番上の行)"
                    className="w-full bg-black border border-zinc-700 text-xs p-2 text-pink-400 placeholder-zinc-600 focus:border-pink-500 outline-none"
                  />
                </div>
                <div>
                  <label className="text-[10px] text-zinc-500 block mb-1">最低レアリティ (MIN RARITY)</label>
                  <div className="flex flex-wrap gap-1">
                    {RARITY_TIERS.map(tier => (
                      <button
                        key={tier.label}
                        onClick={() => setWizRarity(tier.label === wizRarity ? "" : tier.label)}
                        className={clsx(
                          "text-[10px] px-2 py-1 border transition-all",
                          wizRarity === tier.label
                            ? `border-white ${tier.bg} text-white`
                            : "border-zinc-800 bg-zinc-900 text-zinc-500 hover:border-zinc-600"
                        )}
                      >
                        {tier.label}
                      </button>
                    ))}
                  </div>
                </div>
                <button onClick={() => setWizardStep(2)} className="w-full mt-2 bg-zinc-800 hover:bg-zinc-700 text-xs py-1 text-zinc-300">次へ (NEXT) &gt;</button>
              </div>
            ) : (
              <div className="space-y-3 animate-in fade-in slide-in-from-right-4">
                <div className="flex gap-2">
                  <div className="flex-1">
                    <label className="text-[10px] text-zinc-500 block mb-1">支払い上限 (MAX COST)</label>
                    <input
                      type="number"
                      value={wizMaxPrice}
                      onChange={e => setWizMaxPrice(e.target.value)}
                      placeholder="∞"
                      className="w-full bg-black border border-zinc-700 text-xs p-2 text-amber-500 placeholder-zinc-600 focus:border-amber-500 outline-none"
                    />
                  </div>
                  <div className="flex-1">
                    <label className="text-[10px] text-zinc-500 block mb-1">最低利益 (MIN PROFIT)</label>
                    <input
                      type="number"
                      value={wizMinProfit}
                      onChange={e => setWizMinProfit(e.target.value)}
                      placeholder="0"
                      className="w-full bg-black border border-zinc-700 text-xs p-2 text-green-500 placeholder-zinc-600 focus:border-green-500 outline-none"
                    />
                  </div>
                </div>
                <div className="flex gap-2">
                  <button onClick={() => setWizardStep(1)} className="flex-1 bg-zinc-900 hover:bg-zinc-800 text-xs py-2 text-zinc-500 border border-zinc-800">&lt; BACK</button>
                  <button onClick={addRule} className="flex-[2] bg-green-900/30 hover:bg-green-800/50 text-xs py-2 text-green-400 border border-green-600 font-bold">ADD RULE TO LIST</button>
                </div>
              </div>
            )}
          </div>

          {/* ACTIVE RULES */}
          <div className="flex-1 flex flex-col overflow-hidden bg-black/50">
            <div className="px-3 py-2 bg-zinc-950 border-b border-green-900/50 text-xs text-zinc-400 flex justify-between items-center">
              <span>ACTIVE PROTOCOLS ({config.rules.length})</span>
              <Save className="w-3 h-3 cursor-pointer hover:text-white" onClick={() => saveConfig()} />
            </div>
            <div className="flex-1 overflow-y-auto p-2 space-y-2">
              {config.rules.length === 0 && <div className="text-center text-zinc-700 text-xs mt-10 italic">NO RULES DEFINED (ALL PASS)</div>}

              {config.rules.map(rule => {
                const rarityTrigger = RARITY_TIERS.find(r => rule.trigger_text.includes(r.label));
                const displayText = rule.trigger_text.filter(t => !RARITY_TIERS.map(rt => rt.label).includes(t)).join(", ") || "ANY";

                return (
                  <div key={rule.id} className="bg-zinc-900 border border-green-900/50 p-2 flex justify-between items-center group hover:border-green-500 transition-colors">
                    <div className="flex flex-col gap-1 overflow-hidden">
                      <div className="flex items-center gap-2">
                        {rarityTrigger && (
                          <span className={clsx("text-[10px] px-1 rounded-sm", rarityTrigger.bg, rarityTrigger.color)}>
                            {rarityTrigger.label}+
                          </span>
                        )}
                        <span className="text-xs font-bold text-white truncate">{displayText}</span>
                        {rule.target_attribute && <span className="text-[10px] text-pink-400 border border-pink-900 bg-pink-950 px-1 rounded">@{rule.target_attribute}</span>}
                      </div>
                      <div className="text-[10px] text-zinc-500 flex gap-2">
                        <span>&lt; {rule.max_value?.toLocaleString() ?? "∞"}G</span>
                      </div>
                    </div>
                    <button onClick={() => removeRule(rule.id)} className="text-zinc-600 hover:text-red-500 p-2"><Trash2 className="w-4 h-4" /></button>
                  </div>
                )
              })}
            </div>
          </div>

          {/* LOGS */}
          <div className="h-48 border-t border-green-900 flex flex-col bg-zinc-950">
            <div className="h-6 bg-zinc-900 px-2 flex items-center text-[10px] text-zinc-500 border-b border-zinc-800">SYSTEM_LOGS</div>
            <div className="flex-1 overflow-y-auto p-2 font-mono text-[10px] space-y-1">
              {logs.map((l, i) => (
                <div key={i} className="flex gap-2">
                  <span className="text-zinc-600">[{l.timestamp}]</span>
                  <span className={clsx(
                    l.log_type === "Action" ? "text-amber-500 font-bold" :
                      l.log_type === "Logic" ? "text-cyan-500" :
                        l.log_type === "Ocr" ? "text-zinc-500" : "text-green-600"
                  )}>{l.message}</span>
                </div>
              ))}
            </div>
          </div>

        </div>
      </div>

      {/* LIABILITY WAIVER MODAL */}
      {showLiability && (
        <div className="fixed inset-0 z-[100] bg-black/95 flex items-center justify-center p-8 backdrop-blur-md">
          <div className="max-w-2xl w-full bg-zinc-900 border border-red-900 p-8 shadow-2xl shadow-red-900/50">
            <div className="flex items-center gap-4 mb-6 border-b border-zinc-800 pb-4">
              <Shield className="w-12 h-12 text-red-600 animate-pulse" />
              <div>
                <h1 className="text-3xl font-black text-white tracking-tighter">LIABILITY WAIVER</h1>
                <p className="text-red-500 font-mono text-sm">NON-NEGOTIABLE AGREEMENT // 契約の儀式</p>
              </div>
            </div>

            <div className="space-y-4 text-zinc-300 font-mono text-sm h-64 overflow-y-auto mb-8 pr-2 border border-zinc-800 p-4 bg-black/50">
              <p><strong className="text-white">1. NO WARRANTY (無保証):</strong> This software is provided "as is" without warranty of any kind. You assume full responsibility for its use.</p>
              <p><strong className="text-white">2. COMPLIANCE (コンプライアンス):</strong> You agree to use this tool only in accordance with the terms of service of any target application. The creator bears no liability for bans or account suspensions.</p>
              <p><strong className="text-white">3. RISK (リスク):</strong> Automating interactions carries inherent risks. You acknowledge that financial loss or data loss is possible.</p>
              <p><strong className="text-white">4. FINALITY (不可逆性):</strong> By clicking AGREE, you permanently bind yourself to these terms. This decision is recorded.</p>
            </div>

            <div className="flex justify-end gap-4">
              <button
                onClick={() => {
                  invoke('stop_rust_engine');
                  window.close();
                }}
                className="px-6 py-3 text-zinc-500 hover:text-white font-bold transition-colors"
              >
                DECLINE (EXIT)
              </button>
              <button
                onClick={async () => {
                  const newConfig = { ...config, agreed_to_terms: true };
                  setConfig(newConfig);
                  await invoke('update_config', { newConfig });
                  setShowLiability(false);
                }}
                className="px-8 py-3 bg-red-700 hover:bg-red-600 text-white font-black tracking-widest shadow-lg shadow-red-900/20 hover:shadow-red-600/40 transition-all transform hover:scale-105"
              >
                I AGREE (契約する)
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
