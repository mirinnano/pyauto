import json
import os

class ConfigManager:
    def __init__(self, config_path="config.json"):
        self.config_path = config_path
        self.settings = self.load_default_config()
        self.load()

    def load_default_config(self):
        return {
            "target_window": "Fortnite",
            "ocr_region": None,
            "rules": [
                {"trigger_text": ["Slime", "Legendary"], "action_key": "[CLICK]", "cooldown": 2.0},
                {"trigger_text": "START", "action_key": "space", "cooldown": 1.0}
            ],
            "gpu_enabled": True,
            "polling_rate": 0.05,
            "stealth_mode": True,
            "app_title": "System Diagnostic Utility"
        }

    def load(self):
        if os.path.exists(self.config_path):
            with open(self.config_path, "r") as f:
                try:
                    self.settings.update(json.load(f))
                except:
                    pass

    def save(self):
        with open(self.config_path, "w") as f:
            json.dump(self.settings, f, indent=4)
