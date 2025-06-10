import librosa
import librosa.display
import matplotlib.pyplot as plt
import numpy as np

# Load audio
y, sr = librosa.load('babe.wav')  # Works with most formats

# Compute STFT
D = np.abs(librosa.stft(y, n_fft=2048, hop_length=512))

# Convert to dB
DB = librosa.amplitude_to_db(D, ref=np.max)

# Plot
plt.figure(figsize=(12, 6))
librosa.display.specshow(DB, sr=sr, hop_length=512, x_axis='time', y_axis='log', cmap='magma')
plt.colorbar(format='%+2.0f dB')
plt.title('Spectrogram (Log-Frequency)')
plt.tight_layout()
plt.show()

