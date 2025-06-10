import librosa
import numpy as np
import scipy.signal
import matplotlib.pyplot as plt

def detect_drops(audio_path, rmse_threshold=0.05, smoothing_kernel=7):
    # Load audio
    y, sr = librosa.load(audio_path, sr=None)
    
    # Compute RMSE (Root Mean Square Energy)
    rmse = librosa.feature.rms(y=y)[0]
    
    # Smooth RMSE to reduce noise
    rmse_smooth = scipy.signal.medfilt(rmse, kernel_size=smoothing_kernel)
    
    # Find local minima in the smoothed RMSE curve (possible energy drops)
    minima = (np.diff(np.sign(np.diff(rmse_smooth))) > 0).nonzero()[0] + 1
    
    # Filter minima based on threshold - only consider significant drops
    drop_frames = [i for i in minima if rmse_smooth[i] < rmse_threshold]
    drop_times = librosa.frames_to_time(drop_frames, sr=sr)
    
    # Plot the energy with detected drops
    plt.figure(figsize=(14, 6))
    times = librosa.frames_to_time(np.arange(len(rmse)), sr=sr)
    plt.plot(times, rmse, alpha=0.5, label='RMSE')
    plt.plot(times, rmse_smooth, label='Smoothed RMSE', linewidth=2)
    plt.scatter(drop_times, rmse_smooth[drop_frames], color='red', label='Detected Drops')
    plt.xlabel("Time (seconds)")
    plt.ylabel("Energy")
    plt.title("Drop/Breakdown Detection via RMSE Energy")
    plt.legend()
    plt.show()
    
    return drop_times

if __name__ == "__main__":
    audio_path = 'babe.wav'  # Replace with your WAV file path
    drops = detect_drops(audio_path)
    print("Detected possible drops/breakdowns at (seconds):")
    for t in drops:
        print(f"{t:.2f}")

