import time
import random
import re
import json
import base64
import cv2
import os
import numpy as np
from config_manager import ConfigManager
from screen_capture import ScreenCapturer
from ocr_processor import OCRProcessor
from controller import Controller

import sys

class Ledger:
    def __init__(self, filename="ledger.json"):
        self.filename = filename

    def log_transaction(self, item_name, price, confidence=None):
        entry = {
            "timestamp": time.time(),
            "item": item_name,
            "price": price,
            "confidence": confidence,
            "date_str": time.strftime("%Y-%m-%d %H:%M:%S")
        }
        
        try:
            data = []
            if os.path.exists(self.filename):
                with open(self.filename, 'r', encoding='utf-8') as f:
                    try:
                        data = json.load(f)
                    except json.JSONDecodeError:
                        data = []
            
            data.insert(0, entry) # Newest first
            # Keep log size manageable
            if len(data) > 1000:
                data = data[:1000]

            with open(self.filename, 'w', encoding='utf-8') as f:
                json.dump(data, f, indent=2, ensure_ascii=False)
                
            print(f"[LEDGER] Logged: {item_name} @ {price}", flush=True)
        except Exception as e:
            print(f"[LEDGER] Error logging: {e}", flush=True)

class AutoBot:
    def __init__(self, config=None, callback=None):
        print("[DEBUG] AutoBot Initializing...", flush=True)
        self.config = config or ConfigManager()
        self.capturer = ScreenCapturer()
        self.ocr = OCRProcessor(gpu=self.config.settings.get("gpu_enabled", True))
        self.controller = Controller()
        self.active = False
        self.cooldowns = {}
        self.callback = callback
        self.ledger = Ledger() # Initialize Ledger

    def extract_numeric(self, text):
        # Extract numbers like "$25", "100.5", "1,000"
        match = re.search(r"(\d+(?:,\d+)*(?:\.\d+)?)", text)
        if match:
            return float(match.group(1).replace(",", ""))
        return None

    def setup_region(self):
        # Called from Main Thread
        manual = self.config.settings.get("ocr_region")
        target = self.config.settings.get("target_window")
        
        # Determine region geometry safely
        return self.capturer.scan_and_set_region(window_name=target, manual_region=manual)

    def run_loop(self):
        self.active = True
        
        # Thread safety for frame exchange
        import threading
        self._latest_frame = None
        self._frame_lock = threading.Lock()
        
        # Start Capture Thread (Producer)
        def capture_worker():
            print("[DEBUG] Capture Worker Started", flush=True)
            import mss
            try: 
                with mss.mss() as sct:
                    monitor = self.capturer.monitor
                    if not monitor:
                        monitor = sct.monitors[1]
                    
                    print(f"[DEBUG] Capture Monitor: {monitor}", flush=True)

                    while self.active:
                        try:
                            img = sct.grab(monitor)
                            frame = np.array(img)
                            with self._frame_lock:
                                self._latest_frame = frame
                            time.sleep(0.0005) 
                        except Exception as e:
                            print(f"[DEBUG] CapWorker Loop Error: {e}", flush=True)
                            time.sleep(1)
            except Exception as e:
                 print(f"[DEBUG] CapWorker Fatal Error: {e}", flush=True)
        
        capture_thread = threading.Thread(target=capture_worker, daemon=True)
        capture_thread.start()
        
        last_preview_time = 0
        print("[DEBUG] Main Loop Consumer Started", flush=True)
        
        loop_count = 0
        while self.active:
            try:
                # Heartbeat (every 100 frames ~ 3s)
                loop_count += 1
                if loop_count % 100 == 0:
                     print(f"[DEBUG] Heartbeat {loop_count} Frame={self._latest_frame is not None}", flush=True)

                # 30 FPS Target Loop = ~0.033s per cycle
                start_time = time.time()
                # Grab latest frame
                with self._frame_lock:
                    if self._latest_frame is None:
                        time.sleep(0.01)
                        continue
                    frame = self._latest_frame.copy()
                
                # Inference (CPU Intensive)
                results = self.ocr.process(frame)
                
                # Logic Processing (Fast)
                self._process_rules(results, frame)

                # Visual Feed (Throttled to save CPU for OCR)
                now = time.time()
                if self.callback and now - last_preview_time > 0.1: # 10 FPS Preview is enough for human eye, gives CPU to OCR
                    self._send_preview(frame, results)
                    last_preview_time = now
                
                # Dynamic Sleep to maintain ~30 FPS consumer loop without hogging CPU
                elapsed = time.time() - start_time
                target_frame_time = 0.033 # ~30 FPS
                if elapsed < target_frame_time:
                    time.sleep(target_frame_time - elapsed)

            except Exception as e:
                # Log loop errors but don't crash
                if self.callback:
                    self.callback({"type": "log", "data": f"LoopErr: {str(e)}"})
                time.sleep(1)

    def _process_rules(self, results, frame):
        # Global Settings
        global_key = self.config.settings.get("global_action_key", "e")
        base_hold_duration = float(self.config.settings.get("hold_duration", 1.2))
        rules = self.config.settings.get("rules", [])
        
        trigger_action = None
        trigger_details = ""
        trigger_item = ""
        trigger_price = None
        
        # 1. Pre-calculate centers for all results to speed up spatial search
        annotated_results = []
        for r in results:
            box = r['box']
            cx = (box[0][0] + box[2][0]) / 2
            cy = (box[0][1] + box[2][1]) / 2
            r['center'] = (cx, cy)
            annotated_results.append(r)

        for rule in rules:
            trigger_keywords = rule.get("trigger_text", [])
            if isinstance(trigger_keywords, str): trigger_keywords = [trigger_keywords]
            
            max_val = rule.get("max_value")
            min_val = rule.get("min_value")
            cd = rule.get("cooldown", 1.0)
            
            rule_id = str(rule.get("id", trigger_keywords))
            if rule_id in self.cooldowns:
                if time.time() - self.cooldowns[rule_id] < cd:
                    continue

            # Find Keyword Matches
            for res in annotated_results:
                text_upper = res['text'].upper()
                matched_keyword = None
                for k in trigger_keywords:
                    if k.upper() in text_upper:
                        matched_keyword = k
                        break
                
                if not matched_keyword:
                    continue
                    
                # KEYWORD FOUND. Now look for Price/Value LOCALLY.
                print(f"[DEBUG] Matched Keyword: {matched_keyword}", flush=True)
                
                price_match_found = False
                detected_value = None
                
                if max_val is None and min_val is None:
                    print(f"[DEBUG] No Price Constraints -> TRIGGER", flush=True)
                    price_match_found = True
                    trigger_details = f"[{matched_keyword}] (No Price Limit)"
                    trigger_item = matched_keyword
                else:
                    print(f"[DEBUG] Price Constraints Active (Min:{min_val} Max:{max_val}). Searching spatially...", flush=True)
                    found_valid_price = False
                    for p_res in annotated_results:
                        if p_res is res: continue
                        
                        ydiff = abs(p_res['center'][1] - res['center'][1])
                        xdiff = abs(p_res['center'][0] - res['center'][0])
                        
                        if ydiff < 50:
                            if xdiff < 600:
                                val = self.extract_numeric(p_res['text'])
                                if val is not None:
                                    print(f"[DEBUG] Candidate Price found: {val} (X:{xdiff:.1f} Y:{ydiff:.1f})", flush=True)
                                    if max_val is not None and val > max_val: 
                                        print(f"[DEBUG] Val {val} > Max {max_val} -> Skip", flush=True)
                                        continue
                                    if min_val is not None and val < min_val: 
                                        print(f"[DEBUG] Val {val} < Min {min_val} -> Skip", flush=True)
                                        continue
                                    
                                    found_valid_price = True
                                    detected_value = val
                                    break
                    
                    if found_valid_price:
                        price_match_found = True
                        trigger_details = f"[{matched_keyword}] found @ {detected_value}"
                        trigger_item = matched_keyword
                        trigger_price = detected_value
                    else:
                        print(f"[DEBUG] Spatial Search FAILED. No valid price found for {matched_keyword}.", flush=True)

                if price_match_found:
                    trigger_action = rule_id
                    break 
            
            if trigger_action:
                break 

        if trigger_action:
            if self.callback:
                self.callback({"type": "log", "data": f"BUY TRIGGER: {trigger_details}"})
            
            # Phase 3: Financial Intelligence (Ledger)
            self.ledger.log_transaction(trigger_item, trigger_price)

            # Phase 3: Risk Management (Stochastic Timing)
            actual_duration = random.gauss(base_hold_duration, 0.1)
            actual_duration = max(0.5, actual_duration) # Safety floor
            
            print(f"[ANTI-CHEAT] Stochastic Hold: {base_hold_duration}s -> {actual_duration:.3f}s", flush=True)

            self.controller.press_key(global_key, duration=actual_duration)
            self.cooldowns[trigger_action] = time.time()

    def start(self):
        # Ensure imports availability
        import os
        
        if not self.active:
            self.callback({"type": "info", "data": "Starting automation loop..."})
            self.callback({"type": "status", "data": {"isRunning": True, "message": "Engine Started"}})
            self.active = True # Set active state here
            try:
                self.run_loop()
            except Exception as e:
                import traceback
                err = traceback.format_exc()
                print(f"[FATAL] Run Loop Crashed: {e}\n{err}")
                self.callback({"type": "error", "data": f"Engine Crash: {e}"})

    def _send_preview(self, frame, results):
        h, w = frame.shape[:2]
        # Optimize encoding: Resize to 800px (Balance quality/speed)
        preview_w = 800 
        preview_h = int(preview_w * h / w)
        preview_frame = cv2.resize(frame, (preview_w, preview_h), interpolation=cv2.INTER_NEAREST)
        
        _, buffer = cv2.imencode('.jpg', preview_frame, [int(cv2.IMWRITE_JPEG_QUALITY), 60])
        b64_img = base64.b64encode(buffer).decode('utf-8')
        
        self.callback({
            "type": "preview",
            "data": {
                "image": b64_img,
                "results": results,
                "width": w,
                "height": h
            }
        })

        now = time.time()
        if now - getattr(self, '_last_log_time', 0) > 1.0:
            visible_texts = [r['text'] for r in results if r['confidence'] > 0.4]
            if visible_texts:
                 filtered = [t for t in visible_texts if len(t) > 1]
                 summary = " | ".join(filtered[:5])
                 if len(filtered) > 5: summary += "..."
                 self.callback({"type": "log", "data": f"READ: {summary}"})
            self._last_log_time = now
