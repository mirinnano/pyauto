from ultralytics import YOLO
import os

def train_model():
    # 1. Define dataset config (yaml)
    dataset_yaml = """
path: ../training_data  # dataset root dir
train: images/train  # train images (relative to 'path') 
val: images/val  # val images (relative to 'path') 
test:  # test images (optional)

names:
  0: item_icon
  1: enemy
  2: button
"""
    
    os.makedirs("training_data", exist_ok=True)
    with open("training_data/data.yaml", "w") as f:
        f.write(dataset_yaml)

    # 2. Load a model
    model = YOLO("yolov8n.pt")  # load a pretrained model (recommended for training)

    # 3. Train the model
    results = model.train(data="training_data/data.yaml", epochs=100, imgsz=640)

    # 4. Export
    success = model.export(format="onnx")
    print(f"Model exported: {success}")

if __name__ == '__main__':
    train_model()
