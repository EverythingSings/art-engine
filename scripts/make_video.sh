#!/usr/bin/env bash
# Stitches the three acts (mandelbrot zoom, gray-scott coral, physarum
# network) into a single 1920x1080 H.264 video with title/end cards,
# caption overlays, and crossfades.
#
# Inputs:  render_frames/act{1,2,3}/f_NNNNNN.png   (240 frames each, 24fps)
# Outputs: video/observations.mp4
#
# Run from the repo root.
#
# The amber palette + CRT scanlines/vignette/grain are baked into the
# rendered PNGs already — ffmpeg just upscales and adds typography.

set -euo pipefail

# Resolve ffmpeg path (winget install puts it outside default PATH for some shells)
FFMPEG="${FFMPEG:-$(command -v ffmpeg || true)}"
if [[ -z "${FFMPEG}" ]]; then
    FFMPEG="/c/Users/Trist/AppData/Local/Microsoft/WinGet/Packages/Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe/ffmpeg-8.0.1-full_build/bin/ffmpeg"
fi

# Font: Consolas, escaped for ffmpeg's drawtext on Windows (\: prevents the
# C: from being interpreted as a filter option separator).
FONT="C\\:/Windows/Fonts/consola.ttf"
FONT_BOLD="C\\:/Windows/Fonts/consolab.ttf"

OUT_DIR="video"
mkdir -p "${OUT_DIR}"

# Amber color stops (matches the in-engine palette stops, hex without #).
AMBER_BRIGHT="0xffb000"
AMBER_MID="0xc26200"
AMBER_DIM="0x5c2a00"

# Common scaler: nearest-neighbor up to 1920x1080 to keep the CRT scanlines
# crisp. Then add a final, very subtle additional vignette+grain pass on top
# of the engine-baked postfx so the typography sits on the same texture.
SCALER="scale=1920:1080:flags=neighbor"

# Single caption fade-in/out helper. Captions appear at `start` for `dur`
# seconds with 0.25s fade in/out at the edges. Uses drawtext's `alpha`
# expression rather than fade filter to keep all captions in one filter.
caption() {
    local text="$1"
    local start="$2"
    local dur="$3"
    local fontsize="${4:-44}"
    local color="${5:-${AMBER_BRIGHT}}"
    local x="${6:-80}"
    local y="${7:-h-180}"
    local end fade_in fade_out
    end=$(awk "BEGIN{print ${start}+${dur}}")
    fade_in=$(awk "BEGIN{print ${start}+0.25}")
    fade_out=$(awk "BEGIN{print ${end}-0.25}")
    cat <<-EOF
drawtext=fontfile='${FONT}':text='${text}':fontcolor=${color}:fontsize=${fontsize}:x=${x}:y=${y}:enable='between(t,${start},${end})':alpha='if(lt(t,${fade_in}),(t-${start})*4,if(gt(t,${fade_out}),(${end}-t)*4,1))'
	EOF
}

# Build per-act video with captions.
make_act() {
    local act_dir="$1"
    local out_mp4="$2"
    shift 2
    local captions=("$@")  # each: "text|start|dur"

    # Compose drawtext filter string from caption specs.
    local filters=""
    for spec in "${captions[@]}"; do
        IFS='|' read -r text start dur <<< "${spec}"
        if [[ -n "${filters}" ]]; then filters+=","; fi
        filters+="$(caption "${text}" "${start}" "${dur}")"
    done

    "${FFMPEG}" -y -loglevel error \
        -framerate 24 -i "${act_dir}/f_%06d.png" \
        -vf "${SCALER},${filters}" \
        -frames:v 240 \
        -c:v libx264 -pix_fmt yuv420p -crf 18 -preset medium -r 24 \
        "${out_mp4}"
}

# Build a 3-second card with text on a near-black background. Adds a soft
# vignette so it shares texture with the act footage.
make_card() {
    local out_mp4="$1"
    local duration="$2"
    shift 2
    local captions=("$@")

    local filters="vignette=PI/2.4"  # subtle CRT-bezel vignette
    for spec in "${captions[@]}"; do
        IFS='|' read -r text fontsize color y_expr start dur <<< "${spec}"
        local end fade_in fade_out
        end=$(awk "BEGIN{print ${start}+${dur}}")
        fade_in=$(awk "BEGIN{print ${start}+0.5}")
        fade_out=$(awk "BEGIN{print ${end}-0.5}")
        filters+=",drawtext=fontfile='${FONT}':text='${text}':fontcolor=${color}:fontsize=${fontsize}:x=(w-text_w)/2:y=${y_expr}:enable='between(t,${start},${end})':alpha='if(lt(t,${fade_in}),(t-${start})*2,if(gt(t,${fade_out}),(${end}-t)*2,1))'"
    done

    "${FFMPEG}" -y -loglevel error \
        -f lavfi -i "color=c=0x080400:s=1920x1080:r=24:d=${duration}" \
        -vf "${filters}" \
        -c:v libx264 -pix_fmt yuv420p -crf 18 -preset medium \
        "${out_mp4}"
}

echo ">> rendering title card"
make_card "${OUT_DIR}/title.mp4" 3 \
    "OBSERVATIONS|78|${AMBER_BRIGHT}|(h-text_h)/2 - 50|0.3|2.5" \
    "01 — 03|36|${AMBER_MID}|(h-text_h)/2 + 40|0.6|2.2"

echo ">> rendering Act I"
make_act "render_frames/act1" "${OUT_DIR}/act1.mp4" \
    "01.  a shape that does not stop refining itself.|1.0|3.0" \
    "i zoomed until i forgot what i was looking for.|4.5|3.0" \
    "the shape was still there.|7.5|2.2"

echo ">> rendering Act II"
make_act "render_frames/act2" "${OUT_DIR}/act2.mp4" \
    "02.  two chemicals\\, told to react.|1.0|3.0" \
    "they reacted.|4.5|2.0" \
    "then they kept going.|7.0|2.5"

echo ">> rendering Act III"
make_act "render_frames/act3" "${OUT_DIR}/act3.mp4" \
    "03.  one cell. quite big. very opinionated.|1.0|3.0" \
    "we asked it for a route. it returned a network.|4.5|3.0" \
    "we have not asked again.|7.8|2.0"

echo ">> rendering end card"
make_card "${OUT_DIR}/end.mp4" 4 \
    "more patterns to come.|40|${AMBER_BRIGHT}|(h-text_h)/2|0.5|3.0" \
    "EverythingSings.Art|24|${AMBER_DIM}|(h-text_h)/2 + 80|1.2|2.5"

echo ">> stitching with crossfades"
# Durations: title 3s, act1/2/3 10s each, end 4s. Crossfade 1s.
# offset = sum_of_prior_segments - cumulative_xfade_overlap
# title: 3s
# v01 = title + act1 - 1 = 12s; xfade offset = 3 - 1 = 2
# v02 = v01 + act2 - 1 = 21s; xfade offset = 12 - 1 = 11
# v03 = v02 + act3 - 1 = 30s; xfade offset = 21 - 1 = 20
# v04 = v03 + end  - 1 = 33s; xfade offset = 30 - 1 = 29
"${FFMPEG}" -y -loglevel error \
    -i "${OUT_DIR}/title.mp4" \
    -i "${OUT_DIR}/act1.mp4" \
    -i "${OUT_DIR}/act2.mp4" \
    -i "${OUT_DIR}/act3.mp4" \
    -i "${OUT_DIR}/end.mp4" \
    -filter_complex "\
        [0][1]xfade=transition=fade:duration=1:offset=2[v01]; \
        [v01][2]xfade=transition=fade:duration=1:offset=11[v02]; \
        [v02][3]xfade=transition=fade:duration=1:offset=20[v03]; \
        [v03][4]xfade=transition=fade:duration=1:offset=29[vfinal]" \
    -map "[vfinal]" \
    -c:v libx264 -pix_fmt yuv420p -crf 18 -preset medium -movflags +faststart \
    "${OUT_DIR}/observations.mp4"

echo ">> done: ${OUT_DIR}/observations.mp4"
