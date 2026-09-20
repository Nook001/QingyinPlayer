#!/usr/bin/env bash
# Create N tiny WAV files in DEST. Does not touch the user library.
set -euo pipefail

count="${1:?count}"
dest="${2:?dest}"
mkdir -p "$dest"

python3 - "$count" "$dest" <<'PY'
import os, sys
count = int(sys.argv[1])
dest = sys.argv[2]
data_len = 16
wav = bytearray()
wav.extend(b"RIFF")
wav.extend((36 + data_len).to_bytes(4, "little"))
wav.extend(b"WAVE")
wav.extend(b"fmt ")
wav.extend((16).to_bytes(4, "little"))
wav.extend((1).to_bytes(2, "little"))
wav.extend((1).to_bytes(2, "little"))
wav.extend((8000).to_bytes(4, "little"))
wav.extend((16000).to_bytes(4, "little"))
wav.extend((2).to_bytes(2, "little"))
wav.extend((16).to_bytes(2, "little"))
wav.extend(b"data")
wav.extend(data_len.to_bytes(4, "little"))
wav.extend(b"\x00" * data_len)
for i in range(count):
    path = os.path.join(dest, f"{i:05d}.wav")
    with open(path, "wb") as handle:
        handle.write(wav)
print(f"wrote {count} files to {dest}")
PY
