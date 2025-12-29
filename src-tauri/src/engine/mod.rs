pub mod capture;
pub mod input;
pub mod license;
pub mod ocr;

use parking_lot::RwLock;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

use self::capture::{crop_buffer, Region, ScreenCapturer};
use self::input::InputController;
use self::ocr::{OcrData, OcrEngine};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::{imageops, ExtendedColorType, ImageBuffer, ImageEncoder, Rgba};
use regex::Regex;
use serde::{Deserialize, Serialize};
use strsim::normalized_levenshtein;
use tauri::{AppHandle, Emitter};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_0, VK_1, VK_2, VK_3, VK_4, VK_5, VK_6, VK_7, VK_8, VK_9, VK_A, VK_B, VK_C,
    VK_D, VK_E, VK_ESCAPE, VK_F, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6,
    VK_F7, VK_F8, VK_F9, VK_G, VK_H, VK_I, VK_J, VK_K, VK_L, VK_M, VK_N, VK_O, VK_P, VK_Q, VK_R,
    VK_RETURN, VK_S, VK_SPACE, VK_T, VK_TAB, VK_U, VK_V, VK_W, VK_X, VK_Y, VK_Z,
};

#[derive(Clone, Serialize, Debug)]
pub enum LogType {
    System,
    Ocr,
    Logic,
    Action,
}

#[derive(Clone, Serialize, Debug)]
pub struct LogEntry {
    pub timestamp: String,
    pub log_type: LogType,
    pub message: String,
}

#[derive(Clone, Deserialize, Debug)]
pub struct Rule {
    pub id: String,
    pub trigger_text: Vec<String>,
    pub max_value: Option<f32>,
    pub min_value: Option<f32>,
    pub target_attribute: Option<String>,
    pub cooldown: f32,
}

#[derive(Clone, Deserialize, Debug)]
pub struct AppConfig {
    pub target_window: Option<String>,
    pub global_action_key: Option<String>,
    pub hold_duration: Option<f32>,
    pub rules: Option<Vec<Rule>>,
    pub discord_webhook_url: Option<String>,
    pub notify_on_success: Option<bool>,
    pub notify_on_failure: Option<bool>,
    pub notify_on_error: Option<bool>,
}

fn emit_log(app: &AppHandle, log_type: LogType, msg: String) {
    let entry = LogEntry {
        timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
        log_type,
        message: msg,
    };
    let _ = app.emit("log-message", entry);
}

pub struct RustBot {
    active: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
    brain_handle: Option<thread::JoinHandle<()>>,
}

impl RustBot {
    pub fn new() -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            handle: None,
            brain_handle: None,
        }
    }

    pub fn start(&mut self, app_handle: AppHandle, config: AppConfig) {
        if self.active.load(Ordering::SeqCst) {
            return;
        }
        self.active.store(true, Ordering::SeqCst);
        let active_flag = self.active.clone();
        let active_flag_brain = self.active.clone();
        let app_handle_brain = app_handle.clone();
        let brain_config = config.clone();

        // Shared Image Frame (Capture -> Brain)
        // (Data, Width, Height)
        let latest_frame: Arc<RwLock<Option<(Vec<u8>, u32, u32)>>> = Arc::new(RwLock::new(None));
        let brain_frame = latest_frame.clone();

        emit_log(
            &app_handle,
            LogType::System,
            format!("Engine Triggered. PID: {}", std::process::id()),
        );

        // --- 1. Brain Thread (Logic & OCR) ---
        self.brain_handle = Some(thread::spawn(move || {
            emit_log(
                &app_handle_brain,
                LogType::System,
                "Brain (OCR) thread started.".to_string(),
            );

            // Init OCR
            let ocr = match OcrEngine::new() {
                Ok(engine) => {
                    emit_log(
                        &app_handle_brain,
                        LogType::System,
                        "Brain connected to Visual Cortex.".to_string(),
                    );
                    Some(engine)
                }
                Err(e) => {
                    emit_log(
                        &app_handle_brain,
                        LogType::System,
                        format!("Brain Lobotomy Error: {}", e),
                    );
                    None
                }
            };

            // Init Controller (Brain can press too)
            let mut controller = InputController::new();

            // ROI: Configured for "Auto-Buy" detection (Expanded Vertically)
            let roi = Region::new(320, 0, 1280, 1080);

            // let mut loops = 0; // Removed loops check for OCR as it runs continuously as fast as possible
            while active_flag_brain.load(Ordering::SeqCst) {
                // Fetch latest frame
                let frame_opt = {
                    let lock = brain_frame.read();
                    lock.clone()
                };

                if let Some((_data, w, h)) = frame_opt {
                    if let Some(ref engine) = ocr {
                        // 1. CROPPING
                        if let Some(mut cropped_data) = crop_buffer(&_data, w, h, roi) {
                            // 3.5 PRE-PROCESSING (High Contrast Grayscale)
                            preprocess_image(&mut cropped_data);

                            // 4. RECOGNITION
                            // let _start_ocr = Instant::now();
                            match engine.process_frame(&cropped_data, roi.width, roi.height) {
                                Ok(findings) => {
                                    // Vec<OcrData>
                                    // let duration = start_ocr.elapsed();

                                    if !findings.is_empty() {
                                        // Emit OCR Data for Visual Debugging (Bounding Boxes)
                                        let _ = app_handle_brain.emit("ocr-data", &findings);
                                    }
                                    // 4.5 DETECT ATTRIBUTE (Topmost item)
                                    // The topmost detected text is considered the "Attribute"/Title.
                                    let attribute_text = findings
                                        .iter()
                                        .min_by(|a, b| {
                                            a.y.partial_cmp(&b.y)
                                                .unwrap_or(std::cmp::Ordering::Equal)
                                        })
                                        .map(|item| item.text.to_lowercase());

                                    // 5. DYNAMIC LOGIC
                                    if let Some(rules) = &brain_config.rules {
                                        for rule in rules {
                                            for item in findings.iter() {
                                                let text = item.text.to_lowercase(); // Use item.text

                                                // --- Z. ATTRIBUTE CHECK ---
                                                if let Some(req_attr) = &rule.target_attribute {
                                                    // If rule requires attribute, we must match the Topmost text
                                                    if let Some(curr_attr) = &attribute_text {
                                                        if !curr_attr
                                                            .contains(&req_attr.to_lowercase())
                                                        {
                                                            continue;
                                                        }
                                                    } else {
                                                        continue; // Attribute required but none found
                                                    }
                                                }

                                                // --- A. KEYWORD MATCHING (FUZZY) ---
                                                let keyword_match =
                                                    rule.trigger_text.iter().any(|t| {
                                                        let keyword = t.to_lowercase();

                                                        if keyword.contains(' ') {
                                                            // Phrase: Loose substring match (Classic)
                                                            text.contains(&keyword)
                                                        } else {
                                                            // Word: Fuzzy Match (Levenshtein > 0.85)
                                                            text.split(|c: char| {
                                                                !c.is_alphanumeric()
                                                            })
                                                            .any(|word| {
                                                                word == keyword
                                                                    || normalized_levenshtein(
                                                                        word, &keyword,
                                                                    ) > 0.85
                                                            })
                                                        }
                                                    });

                                                if !keyword_match {
                                                    continue;
                                                }

                                                // --- B. PRICE CHECK (REGEX) ---
                                                let price_regex = Regex::new(r"[\d,\.]+").unwrap();

                                                let price_satisfied = if rule.max_value.is_some()
                                                    || rule.min_value.is_some()
                                                {
                                                    let mut found_valid_price = false;
                                                    for cap in price_regex.find_iter(&item.text) {
                                                        let num_str = cap.as_str().replace(',', "");
                                                        if let Ok(val) = num_str.parse::<f32>() {
                                                            let min_ok = rule
                                                                .min_value
                                                                .map_or(true, |min| val >= min);
                                                            let max_ok = rule
                                                                .max_value
                                                                .map_or(true, |max| val <= max);
                                                            if min_ok && max_ok {
                                                                found_valid_price = true;
                                                                break;
                                                            }
                                                        }
                                                    }
                                                    found_valid_price
                                                } else {
                                                    true
                                                };

                                                if !price_satisfied {
                                                    continue;
                                                }

                                                // --- C. TRIGGER ACTION ---
                                                emit_log(
                                                    &app_handle_brain,
                                                    LogType::Logic,
                                                    format!(
                                                        "Rule '{}' MATCHED. Text: '{}'",
                                                        rule.id, text
                                                    ),
                                                );

                                                // Trigger Webhook (Async spawn)
                                                if let Some(webhook_url) =
                                                    &brain_config.discord_webhook_url
                                                {
                                                    if !webhook_url.is_empty() {
                                                        let url = webhook_url.clone();
                                                        let msg_text = format!(
                                                            "**SNIPED!**\nItem: {}\nRule: {}",
                                                            item.text, rule.id
                                                        );
                                                        let _sc = app_handle_brain.clone();
                                                        thread::spawn(move || {
                                                            let payload = serde_json::json!({
                                                                "content": null,
                                                                "embeds": [{
                                                                    "title": "⚡ ITEM SECURED ⚡",
                                                                    "description": msg_text,
                                                                    "color": 5763719,
                                                                    "footer": { "text": "Antigravity V4" }
                                                                }],
                                                                "username": "ANTIGRAVITY BOT",
                                                                "avatar_url": "https://i.imgur.com/4M34hi2.png"
                                                            });
                                                            let _ =
                                                                ureq::post(&url).send_json(payload);
                                                        });
                                                    }
                                                }

                                                // Parse Key
                                                let key_str = brain_config
                                                    .global_action_key
                                                    .as_deref()
                                                    .unwrap_or("e");
                                                let vk = parse_key(key_str);
                                                let duration =
                                                    brain_config.hold_duration.unwrap_or(1.2);
                                                let duration_ms = (duration * 1000.0) as u64;

                                                emit_log(
                                                    &app_handle_brain,
                                                    LogType::Action,
                                                    format!(
                                                        "PRESS: '{}' ({}ms)",
                                                        key_str, duration_ms
                                                    ),
                                                );
                                                controller.long_press_key(vk, duration_ms);
                                                emit_log(
                                                    &app_handle_brain,
                                                    LogType::Action,
                                                    format!("RELEASED: '{}'", key_str),
                                                );

                                                thread::sleep(Duration::from_millis(1500));
                                            }
                                        }
                                    }
                                }
                                Err(e) => emit_log(
                                    &app_handle_brain,
                                    LogType::System,
                                    format!("OCR Error: {}", e),
                                ),
                            }
                        }
                    }
                }

                // loops += 1;
                thread::sleep(Duration::from_millis(50));
            }
            emit_log(
                &app_handle_brain,
                LogType::System,
                "Brain thread stopped.".to_string(),
            );
        }));

        // --- 2. Body Thread (Capture & Input) ---
        let app_handle_body = app_handle.clone();
        let app_handle_stream = app_handle.clone();

        let body_frame_clone = latest_frame.clone();

        self.handle = Some(thread::spawn(move || {
            emit_log(
                &app_handle_body,
                LogType::System,
                "Body thread started.".to_string(),
            );
            let mut capturer = ScreenCapturer::new();

            let mut loops: u64 = 0;
            let mut last_log = Instant::now();
            let target_frame_time = Duration::from_micros(22222); // ~45 FPS

            while active_flag.load(Ordering::SeqCst) {
                let start = Instant::now();

                // Capture
                match capturer.capture_region(0, 0, 1920, 1080) {
                    Ok(pixels) => {
                        let w = 1920;
                        let h = 1080;
                        // Update Brain's view
                        {
                            if let Some(mut lock) = body_frame_clone.try_write() {
                                *lock = Some((pixels.clone(), w, h));
                            }
                        }

                        // Stream to Frontend (Extreme Optimization: Manual Subsampling)
                        // Target: 480x270 (1/4th Scale) -> ~120KB Raw -> ~5KB JPEG
                        if loops % 2 == 0 {
                            // Manual Downscale 4x + BGR->RGB Swap (Zero intermediate allocation)
                            let target_w = 480;
                            let target_h = 270;
                            let mut small_buffer =
                                Vec::with_capacity((target_w * target_h * 3) as usize);

                            // Stride Calculation
                            // Source width 1920
                            // Skip 4 pixels horizontal, 4 pixels vertical

                            for y in 0..target_h {
                                let src_y = y * 4;
                                let row_start = (src_y * 1920 * 4) as usize;
                                for x in 0..target_w {
                                    let src_x = x * 4;
                                    let idx = row_start + (src_x * 4) as usize;

                                    if idx + 2 < pixels.len() {
                                        let b = pixels[idx];
                                        let g = pixels[idx + 1];
                                        let r = pixels[idx + 2];
                                        // Push RGB
                                        small_buffer.push(r);
                                        small_buffer.push(g);
                                        small_buffer.push(b);
                                    } else {
                                        // Padding if out of bounds (shouldn't happen with correct math)
                                        small_buffer.push(0);
                                        small_buffer.push(0);
                                        small_buffer.push(0);
                                    }
                                }
                            }

                            // Encode small buffer as JPEG (Quality 50 is plenty for preview)
                            let mut jpeg_buffer = Vec::new();
                            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                                &mut jpeg_buffer,
                                50,
                            );

                            // Use RgbImage to wrap our raw buffer
                            if let Some(img) = ImageBuffer::<image::Rgb<u8>, Vec<u8>>::from_raw(
                                target_w,
                                target_h,
                                small_buffer,
                            ) {
                                if let Ok(_) = encoder.encode_image(&img) {
                                    let b64 = BASE64.encode(&jpeg_buffer);
                                    let _ = app_handle_stream.emit("frame-update", b64);
                                }
                            }
                        }
                    }
                    Err(e) => emit_log(
                        &app_handle_body,
                        LogType::System,
                        format!("Capture error: {}", e),
                    ),
                }

                // Stats & Timing
                loops += 1;
                if last_log.elapsed() >= Duration::from_secs(5) {
                    emit_log(
                        &app_handle_body,
                        LogType::System,
                        format!("Heartbeat: {} FPS", loops / 5),
                    );
                    loops = 0;
                    last_log = Instant::now();
                }

                let elapsed = start.elapsed();
                if elapsed < target_frame_time {
                    thread::sleep(target_frame_time - elapsed);
                }
            }
            emit_log(
                &app_handle_body,
                LogType::System,
                "Body thread stopped.".to_string(),
            );
        }));
    }

    pub fn stop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        if let Some(h) = self.brain_handle.take() {
            let _ = h.join();
        }
    }
}

fn parse_key(k: &str) -> VIRTUAL_KEY {
    match k.to_lowercase().as_str() {
        "a" => VK_A,
        "b" => VK_B,
        "c" => VK_C,
        "d" => VK_D,
        "e" => VK_E,
        "f" => VK_F,
        "g" => VK_G,
        "h" => VK_H,
        "i" => VK_I,
        "j" => VK_J,
        "k" => VK_K,
        "l" => VK_L,
        "m" => VK_M,
        "n" => VK_N,
        "o" => VK_O,
        "p" => VK_P,
        "q" => VK_Q,
        "r" => VK_R,
        "s" => VK_S,
        "t" => VK_T,
        "u" => VK_U,
        "v" => VK_V,
        "w" => VK_W,
        "x" => VK_X,
        "y" => VK_Y,
        "z" => VK_Z,
        "0" => VK_0,
        "1" => VK_1,
        "2" => VK_2,
        "3" => VK_3,
        "4" => VK_4,
        "5" => VK_5,
        "6" => VK_6,
        "7" => VK_7,
        "8" => VK_8,
        "9" => VK_9,
        "f1" => VK_F1,
        "f2" => VK_F2,
        "f3" => VK_F3,
        "f4" => VK_F4,
        "f5" => VK_F5,
        "f6" => VK_F6,
        "f7" => VK_F7,
        "f8" => VK_F8,
        "f9" => VK_F9,
        "f10" => VK_F10,
        "f11" => VK_F11,
        "f12" => VK_F12,
        "space" => VK_SPACE,
        "enter" => VK_RETURN,
        "tab" => VK_TAB,
        "esc" => VK_ESCAPE,
        _ => VK_E,
    }
}

fn preprocess_image(data: &mut [u8]) {
    // Pass 1: Analyze Stats (Min, Max, Avg)
    let mut min_val: f32 = 255.0;
    let mut max_val: f32 = 0.0;
    let mut total_gray: f32 = 0.0;
    let mut count: f32 = 0.0;

    // Sampling stride for speed
    for chunk in data.chunks_exact(4) {
        let b = chunk[0] as f32;
        let g = chunk[1] as f32;
        let r = chunk[2] as f32;
        // Use standard Rec.601 for grayscale
        let gray = 0.299 * r + 0.587 * g + 0.114 * b;

        if gray < min_val {
            min_val = gray;
        }
        if gray > max_val {
            max_val = gray;
        }
        total_gray += gray;
        count += 1.0;
    }

    let avg_gray = if count > 0.0 {
        total_gray / count
    } else {
        128.0
    };
    let range = max_val - min_val;

    // Heuristic: Dark Mode if Average < 100
    let invert = avg_gray < 100.0;

    // Pass 2: Apply Auto-Levels & Contrast
    for chunk in data.chunks_exact_mut(4) {
        let b = chunk[0] as f32;
        let g = chunk[1] as f32;
        let r = chunk[2] as f32;
        let mut gray = 0.299 * r + 0.587 * g + 0.114 * b;

        // 1. Invert if needed (Dark text on light bg is best for OCR)
        // If background is dark (invert=true), we make it light (255-gray).
        // Text (bright) becomes dark.
        if invert {
            gray = 255.0 - gray;
            // Note: We should technically flip min/max too if we want perfect normalization logic below,
            // but simpler to normalize first then invert, OR just apply logic blindly.
            // Let's inverted gray first.
        }

        // 2. Auto-Levels (Normalization)
        // Adjust min/max for the inversion if applied?
        // Actually, if we invert, the Histogram flips. min becomes (255-max), max becomes (255-min).
        // Let's recalculate logical range.
        let (eff_min, eff_max) = if invert {
            (255.0 - max_val, 255.0 - min_val)
        } else {
            (min_val, max_val)
        };

        let eff_range = eff_max - eff_min;

        // Avoid div by zero
        let normalized = if eff_range > 10.0 {
            (gray - eff_min) / eff_range
        } else {
            // Flat image, assume it's already "good" or just mid
            0.5
        };

        // 3. Contrast Boost (Sigmoid-ish or Multiplier)
        // Clip to 0.0 - 1.0
        let clipped = normalized.clamp(0.0, 1.0);

        // Final Output (0-255)
        let final_gray = (clipped * 255.0) as u8;

        chunk[0] = final_gray;
        chunk[1] = final_gray;
        chunk[2] = final_gray;
    }
}
