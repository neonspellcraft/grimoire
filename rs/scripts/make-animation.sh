#!/usr/bin/env bash
# Turns a video into one of Grimoire's frame animations (.grma): crops away black bars, keeps the frame rate,
# and packs the frames as JPEGs into a single file the app plays on a loop.
#
# Usage: scripts/make-animation.sh input.mp4 output.grma [quality 2-31, lower is better; default 5]
#
# Format: "GRMA", u32 version (1), u32 frames-per-second × 1000, u32 width, u32 height, u32 frame count,
# then a u32 byte length per frame, then the JPEGs back to back. All integers little-endian.
set -euo pipefail
IN="$1"; OUT="$2"; QUALITY="${3:-5}"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT

# Black bars: the crop ffmpeg's cropdetect finds most often across the clip.
CROP="$(ffmpeg -hide_banner -i "$IN" -vf cropdetect=24:2:0 -f null - 2>&1 | grep -o 'crop=[0-9:]*' | sort | uniq -c | sort -rn | head -1 | awk '{print $2}')"
FPS="$(ffprobe -v error -select_streams v:0 -show_entries stream=r_frame_rate -of csv=p=0 "$IN")"
echo "crop: ${CROP:-none}, frame rate: $FPS"
ffmpeg -v error -i "$IN" -vf "${CROP:-null}" -q:v "$QUALITY" "$TMP/%05d.jpg"

python3 -I - "$TMP" "$OUT" "$FPS" <<'PY'
import glob, struct, sys
folder, out, fps = sys.argv[1], sys.argv[2], sys.argv[3]
num, _, den = fps.partition("/")
milli_fps = round(float(num) / float(den or 1) * 1000)
frames = [open(p, "rb").read() for p in sorted(glob.glob(folder + "/*.jpg"))]
def jpeg_size(data):
    i = 2
    while i < len(data):
        marker, length = data[i + 1], struct.unpack(">H", data[i + 2:i + 4])[0]
        if marker in (0xC0, 0xC1, 0xC2):
            h, w = struct.unpack(">HH", data[i + 5:i + 9])
            return w, h
        i += 2 + length
    raise SystemExit("not a JPEG")
width, height = jpeg_size(frames[0])
with open(out, "wb") as f:
    f.write(b"GRMA" + struct.pack("<5I", 1, milli_fps, width, height, len(frames)))
    f.write(b"".join(struct.pack("<I", len(frame)) for frame in frames))
    f.write(b"".join(frames))
print(f"wrote {out}: {len(frames)} frames, {width}x{height} at {milli_fps / 1000:g} fps")
PY
