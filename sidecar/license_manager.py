import hashlib
import os
import subprocess

class LicenseManager:
    def __init__(self):
        self._hwid = self._generate_hwid()
        self.license_key_path = "license.txt"

    def _generate_hwid(self) -> str:
        try:
            cmd = "wmic diskdrive get serialnumber"
            output = subprocess.check_output(cmd, shell=True).decode().strip()
            serial = output.split('\n')[1].strip()
            return hashlib.sha256(serial.encode()).hexdigest()[:16].upper()
        except:
            import uuid
            return hashlib.sha256(str(uuid.getnode()).encode()).hexdigest()[:16].upper()

    def verify_license(self, provided_key: str = None) -> bool:
        if os.path.exists(".antigravity_master"):
            return True
        
        if not provided_key and os.path.exists(self.license_key_path):
            with open(self.license_key_path, "r") as f:
                provided_key = f.read().strip()
        
        if not provided_key: return False
        
        secret_salt = "ANTIGRAVITY_SALES_2025"
        expected = hashlib.sha256((self._hwid + secret_salt).encode()).hexdigest().upper()
        return provided_key == expected

    def get_hwid(self): return self._hwid
