import numpy as np
from scipy.io import wavfile
import matplotlib.pyplot as plt

# Step 1: Read the audio file
sample_rate, data = wavfile.read('babe.wav')

# If stereo, take one channel
if len(data.shape) > 1:
    data = data[:, 0]

# Step 2: Apply Fourier Transform
N = len(data)
frequencies = np.fft.fftfreq(N, d=1/sample_rate)
fft_magnitude = np.abs(np.fft.fft(data))

# Use only the positive frequencies
half_N = N // 2
frequencies = frequencies[:half_N]
fft_magnitude = fft_magnitude[:half_N]

# Step 3: Plot the spectrum
plt.figure(figsize=(12, 6))
plt.plot(frequencies, fft_magnitude, color='blue')
plt.title('Frequency Spectrum')
plt.xlabel('Frequency (Hz)')
plt.ylabel('Magnitude')
plt.grid(True)
plt.tight_layout()
plt.show()

