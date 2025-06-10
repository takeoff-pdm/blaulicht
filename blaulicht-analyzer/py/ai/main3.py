import openl3
import soundfile as sf

# Load your audio file
audio, sr = sf.read("babe.wav")

# Extract embeddings
embeddings, timestamps = openl3.get_audio_embedding(audio, sr,
                                                    input_repr="mel256",
                                                    content_type="music",
                                                    embedding_size=512)

print(embeddings.shape)  # e.g., (n_frames, 512)

