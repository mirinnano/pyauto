use serde::Serialize;
use windows::Foundation::Collections::IVectorView;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::{OcrEngine as WinOcrEngine, OcrLine, OcrResult};
use windows::Storage::Streams::DataWriter; // Removed InMemoryRandomAccessStream if unused or use it?
                                           // Actually we need InMemoryRandomAccessStream for the way we did it before?
                                           // No, previously we used TryCreateFromUsing... wait.
                                           // Let's stick to the method that worked: DataWriter to IBuffer.
use windows::core::HSTRING;
use windows::Storage::Streams::InMemoryRandomAccessStream;

#[derive(Serialize, Debug, Clone)]
pub struct OcrData {
    pub text: String,
    pub x: f32, // Bounding Box X
    pub y: f32, // Bounding Box Y
    pub w: f32, // Width
    pub h: f32, // Height
}

// Ensure thread safety for OcrEngine (MTA requirement)
pub struct OcrEngine {
    engine: WinOcrEngine,
}
unsafe impl Send for OcrEngine {}
unsafe impl Sync for OcrEngine {}

impl OcrEngine {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize for English (or system default)
        let lang_code = HSTRING::from("en-US");
        // Check availability strictly? Or just try create.

        // TryCreateFromLanguage returns Result<OcrEngine> in recent windows-rs versions, or we handle error.
        let lang_obj = Language::CreateLanguage(&lang_code)?;

        let engine = match WinOcrEngine::TryCreateFromLanguage(&lang_obj) {
            Ok(e) => e,
            Err(_) => {
                println!("[OCR] en-US not found, trying system default...");
                WinOcrEngine::TryCreateFromUserProfileLanguages()?
            }
        };

        println!("[OCR] Windows Native Engine Initialized.");
        Ok(Self { engine })
    }

    pub fn process_frame(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<OcrData>, Box<dyn std::error::Error>> {
        // Create IBuffer from slice using CryptographicBuffer
        // (Must match Cargo.toml features: Security_Cryptography)
        let buffer =
            windows::Security::Cryptography::CryptographicBuffer::CreateFromByteArray(image_data)?;

        // SoftwareBitmap
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            width as i32,
            height as i32,
        )?;

        // Synchronous wrapper for Async operation
        let ocr_result = self.engine.RecognizeAsync(&bitmap)?.get()?;

        let mut findings = Vec::new();
        let lines = ocr_result.Lines()?;
        let count = lines.Size()?;

        for i in 0..count {
            let line = lines.GetAt(i)?;
            // Flatten to words for precise boxes
            let words = line.Words()?;
            let word_count = words.Size()?;

            for j in 0..word_count {
                let word = words.GetAt(j)?;
                let text = word.Text()?.to_string();
                let rect = word.BoundingRect()?;

                findings.push(OcrData {
                    text,
                    x: rect.X as f32,
                    y: rect.Y as f32,
                    w: rect.Width as f32,
                    h: rect.Height as f32,
                });
            }
        }

        Ok(findings)
    }
}
