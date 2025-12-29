# import easyocr (Disabled for speed)
import cv2
import numpy as np
from rapidocr_onnxruntime import RapidOCR

class OCRProcessor:
    def __init__(self, gpu=True):
        # ONNX Runtime is extremely fast even on CPU, but GPU is "God Mode"
        print(f"[OCR] Initializing RapidOCR (ONNX/C++ Backend) [GPU Requested: {gpu}]...")
        
        # Explicitly requesting CUDA providers
        # Note: Requires onnxruntime-gpu and correct CUDA/cuDNN installed on host
        try:
            self.engine = RapidOCR(det_use_cuda=gpu, cls_use_cuda=gpu, rec_use_cuda=gpu)
            print("[OCR] Engine initialized with GPU flags.")
        except Exception as e:
            print(f"[OCR] GPU Init failed (Missing CUDA?), falling back to CPU: {e}")
            self.engine = RapidOCR()

    def pre_process(self, image):
        """Enhances image for better OCR results with performance in mind."""
        h, w = image.shape[:2]
        
        # 2x Upscaling with Linear (faster than Cubic)
        upscaled = cv2.resize(image, (w*2, h*2), interpolation=cv2.INTER_LINEAR)
        
        # Convert to gray
        gray = cv2.cvtColor(upscaled, cv2.COLOR_BGR2GRAY)
        
        # Fast Contrast Enhancement (CLAHE)
        clahe = cv2.createCLAHE(clipLimit=2.0, tileGridSize=(4,4))
        enhanced = clahe.apply(gray)
        
        # PADDING: Essential for OCR accuracy near edges
        # Add 10px white border
        # Use scalar 255 for grayscale
        padded = cv2.copyMakeBorder(enhanced, 10, 10, 10, 10, cv2.BORDER_CONSTANT, value=255)
        
        # RapidOCR prefers BGR/RGB
        padded_bgr = cv2.cvtColor(padded, cv2.COLOR_GRAY2BGR)
        
        return padded_bgr, 2.0, 10

    def process(self, image):
        processed_img, scale, padding = self.pre_process(image)
        
        # RapidOCR returns: [[[[x1, y1], [x2, y2], ...], text, confidence], ...]
        # usage: result, elapse = engine(img)
        result, _ = self.engine(processed_img)
        
        output = []
        if result:
            for item in result:
                bbox, text, prob = item
                # bbox is list of lists
                
                # Adjust coordinates: (coord - padding) / scale
                clean_bbox = [[float((coord - padding) / scale) for coord in pt] for pt in bbox]
                output.append({
                    "text": text,
                    "box": clean_bbox,
                    "confidence": float(prob)
                })
        return output
