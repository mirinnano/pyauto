import pydirectinput
import time
import random

class Controller:
    def __init__(self):
        # Fail-safe: moving mouse to corner aborts
        pydirectinput.FAILSAFE = True

    def press_key(self, key: str, duration: float = 0.1):
        """Presses and releases a key with a randomized duration."""
        actual_duration = duration * random.uniform(0.85, 1.15)
        try:
            pydirectinput.keyDown(key)
            time.sleep(actual_duration)
            pydirectinput.keyUp(key)
        except Exception as e:
            print(f"Input Error: {e}")

    def human_like_click(self, x, y):
        """Moves to and clicks a target with slight randomization."""
        # Add tiny jitter to click position
        jx = x + random.randint(-2, 2)
        jy = y + random.randint(-2, 2)
        
        # Duration for movement simulation
        pydirectinput.moveTo(jx, jy)
        time.sleep(random.uniform(0.05, 0.15))
        pydirectinput.click()
