import sys
import json
import threading
from main import AutoBot
from config_manager import ConfigManager

def emit_event(event_type, data):
    """Sends JSON events to Tauri via stdout."""
    print(json.dumps({"type": event_type, "data": data}))
    sys.stdout.flush()

def bot_callback(event):
    """Callback triggered by the bot loop."""
    print(json.dumps(event))
    sys.stdout.flush()

class Bridge:
    def __init__(self):
        self.bot = None
        self.bot_thread = None

    def listen(self):
        for line in sys.stdin:
            try:
                msg = json.loads(line)
                action = msg.get("action")
                
                if action == "start":
                    config_data = msg.get("config", {})
                    config = ConfigManager()
                    config.settings.update(config_data)
                    
                    self.bot = AutoBot(config=config, callback=bot_callback)
                    if self.bot.setup_region():
                        emit_event("status", {"isRunning": True, "message": "Engine Started"})
                        self.bot_thread = threading.Thread(target=self.bot.run_loop, daemon=True)
                        self.bot_thread.start()
                    else:
                        emit_event("status", {"isRunning": False, "message": "Window Not Found"})
                
                elif action == "stop":
                    if self.bot:
                        self.bot.active = False
                        emit_event("status", {"isRunning": False, "message": "Engine Stopped"})
            except Exception as e:
                emit_event("error", str(e))

if __name__ == "__main__":
    try:
        print(json.dumps({"type": "info", "data": "Bridge listener starting..."}), flush=True)
        bridge = Bridge()
        bridge.listen()
    except Exception as e:
        import traceback
        error_msg = f"CRITICAL_CRASH: {str(e)}\n{traceback.format_exc()}"
        print(json.dumps({"type": "error", "data": error_msg}), flush=True)
        sys.exit(1)
