import json, os, subprocess
root = "models/songformer_hf"
t = json.load(open("tmp/songformer/tree.json"))
for f in t:
    if f.get("type") != "file" or f["path"].endswith(".png"):
        continue
    p = os.path.join(root, f["path"])
    if os.path.exists(p) and os.path.getsize(p) == f.get("size"):
        continue
    os.makedirs(os.path.dirname(p) or ".", exist_ok=True)
    subprocess.run(["curl", "-sSL", "--retry", "3", "-o", p, "https://huggingface.co/ASLP-lab/SongFormer/resolve/main/" + f["path"]], check=True)
    print(f["path"], os.path.getsize(p), flush=True)
print("done", flush=True)
