import numpy as np
import sounddevice as sd
import tensorflow_hub as hub
import tensorflow as tf
import librosa
import time

# Load YAMNet model
yamnet_model = hub.load('https://tfhub.dev/google/yamnet/1')

# Load class names
class_map_path = tf.keras.utils.get_file(
    'yamnet_class_map.csv',
    'https://raw.githubusercontent.com/tensorflow/models/master/research/audioset/yamnet/yamnet_class_map.csv'
)
with open(class_map_path, 'r') as f:
    class_names = [line.strip().split(',')[2] for line in f.readlines()[1:]]

# Parameters
SAMPLE_RATE = 16000
DURATION = 1.0  # seconds
BLOCK_SIZE = int(SAMPLE_RATE * DURATION)

print("🎧 Listening for real-time classification...")

buffer = np.zeros(BLOCK_SIZE, dtype=np.float32)

def audio_callback(indata, frames, time_info, status):
    global buffer
    indata = indata[:, 0]  # mono
    buffer = np.roll(buffer, -frames)
    buffer[-frames:] = indata

# Start input stream
stream = sd.InputStream(callback=audio_callback, channels=1, samplerate=SAMPLE_RATE, blocksize=1024)
stream.start()

try:
    while True:
        # Copy buffer to avoid threading issues
        audio_chunk = np.copy(buffer)

        # Run YAMNet
        scores, embeddings, spectrogram = yamnet_model(audio_chunk)
        prediction = np.mean(scores, axis=0)  # average over time frames
        top_class = class_names[np.argmax(prediction)]
        confidence = prediction[np.argmax(prediction)]

        print(f"🔊 Detected: {top_class} ({confidence:.2f})")

        time.sleep(1.0)  # classify every 1 second

except KeyboardInterrupt:
    print("\nStopping...")
    stream.stop()

