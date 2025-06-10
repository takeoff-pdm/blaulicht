import musicnn.extractor as mne

# Replace this with your actual audio path
audio_path = 'your_audio.wav'

# Run musicnn
tags, scores, timestamps, features = mne.extractor(audio_path, model='MSD_musicnn')

# Print top 10 tags
print("Top Tags:")
for tag, score in sorted(zip(tags, scores.mean(axis=0)), key=lambda x: x[1], reverse=True)[:10]:
    print(f"{tag}: {score:.3f}")

