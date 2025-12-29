use rand::Rng;
use std::thread;
use std::time::Duration;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, VIRTUAL_KEY,
};

pub struct InputController {
    rng: rand::rngs::ThreadRng,
}

impl InputController {
    pub fn new() -> Self {
        Self {
            rng: rand::thread_rng(),
        }
    }

    pub fn click_mouse_left(&mut self) {
        // Stochastic delay before
        self.random_sleep(20, 50);

        self.send_mouse_input(MOUSEEVENTF_LEFTDOWN);
        self.random_sleep(50, 100); // Hold time
        self.send_mouse_input(MOUSEEVENTF_LEFTUP);
    }

    pub fn press_key(&mut self, vk: VIRTUAL_KEY) {
        // Stochastic delay
        self.random_sleep(20, 50);

        self.send_key_input(vk, false); // Press
        self.random_sleep(50, 120); // Human hold time
        self.send_key_input(vk, true); // Release
    }

    pub fn long_press_key(&mut self, vk: VIRTUAL_KEY, duration_ms: u64) {
        self.random_sleep(20, 50);
        self.send_key_input(vk, false); // Down

        // Hold for duration + small jitter
        let jitter = self.rng.gen_range(0..=100);
        thread::sleep(Duration::from_millis(duration_ms + jitter));

        self.send_key_input(vk, true); // Up
    }

    fn send_mouse_input(
        &self,
        flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
    ) {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    }

    fn send_key_input(&self, vk: VIRTUAL_KEY, key_up: bool) {
        let flags = if key_up {
            KEYEVENTF_KEYUP
        } else {
            windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
        };
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    }

    fn random_sleep(&mut self, min_ms: u64, max_ms: u64) {
        let ms = self.rng.gen_range(min_ms..=max_ms);
        thread::sleep(Duration::from_millis(ms));
    }
}
