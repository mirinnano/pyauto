import mss
import numpy as np
import pygetwindow as gw

class ScreenCapturer:
    def __init__(self):
        self.monitor = None
        self.session = None

    def scan_and_set_region(self, window_name=None, manual_region=None):
        """
        Determines the capture region. Safe to call from any thread (Main Thread).
        Uses a temporary MSS instance to get monitor details if needed.
        """
        with mss.mss() as sct:
            if manual_region:
                self.monitor = manual_region
                return True
            
            if window_name:
                try:
                    windows = gw.getWindowsWithTitle(window_name)
                    if windows:
                        win = windows[0]
                        # Handle maximized/fullscreen windows specifically
                        # Sometimes borders usually need minor adjustment, but for fullscreen we take raw
                        self.monitor = {
                            "top": win.top,
                            "left": win.left,
                            "width": win.width,
                            "height": win.height
                        }
                        # Safety check for negative coords (minimized)
                        if self.monitor["left"] < 0: self.monitor["left"] = 0
                        if self.monitor["top"] < 0: self.monitor["top"] = 0
                        return True
                except Exception as e:
                    print(f"Window search error: {e}")
            
            # Fallback to primary monitor (Fullscreen optimization)
            self.monitor = sct.monitors[1]
            return True

    def start_session(self):
        """Initializes the MSS instance for the CURRENT thread (Worker Thread)."""
        if self.session:
            self.session.close()
        self.session = mss.mss()

    def stop_session(self):
        if self.session:
            self.session.close()
            self.session = None

    # King-Alpha Implementation
    def capture(self):
        """Captures the defined region using the thread-local session."""
        if not self.session:
            self.start_session()
            
        # Ensure monitor is set
        if not self.monitor:
            self.monitor = self.session.monitors[1]

        # Grab frame
        try:
            img = self.session.grab(self.monitor)
            return np.array(img)
        except Exception as e:
            # Re-init session on failure (sometimes happens with resolution change)
            print(f"Capture error, re-initializing: {e}")
            self.start_session()
            return np.array(self.session.grab(self.monitor))
