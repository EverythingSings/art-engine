#!/usr/bin/env bash
# Stitches the three new-engine motion sequences (DLA growth, Lorenz orbit
# emerging, Particles vortex evolving) into a 1080x1920 vertical short
# titled "OBSERVATIONS / 04—06" to extend the original transmission arc.
#
# Inputs:  render_frames/{dla_grow,lorenz_grow,parts_swirl}/f_NNNNNN.png   (240 frames each)
# Output:  video/observations_04_06.mp4
#
# Run from the repo root.

set -euo pipefail

FFMPEG="${FFMPEG:-$(command -v ffmpeg || true)}"
if [[ -z "${FFMPEG}" ]]; then
    FFMPEG="/c/Users/Trist/AppData/Local/Microsoft/WinGet/Packages/Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe/ffmpeg-8.0.1-full_build/bin/ffmpeg"
fi

FONT="C\\:/Windows/Fonts/consola.ttf"
OUT_DIR="video"
mkdir -p "${OUT_DIR}"

AMBER_BRIGHT="0xffb000"
AMBER_MID="0xc26200"
AMBER_DIM="0x5c2a00"

SCALER="scale=1080:1920:flags=neighbor"
ACT_FRAMES=240    # 10s @ 24fps
TITLE_DURATION=3
END_DURATION=3
XFADE=0.5
TOTAL_DURATION=34   # 3 + 10 + 10 + 10 + 3 - 4*0.5 = 34

# Per-act caption renderer.
make_act() {
    local act_dir="$1"
    local out_mp4="$2"
    shift 2
    local captions=("$@")
    local filters="${SCALER}"
    for spec in "${captions[@]}"; do
        IFS='|' read -r text start dur fontsize y_expr <<< "${spec}"
        : "${fontsize:=54}"
        : "${y_expr:=h*0.78}"
        local end fade_in fade_out
        end=$(awk "BEGIN{print ${start}+${dur}}")
        fade_in=$(awk "BEGIN{print ${start}+0.2}")
        fade_out=$(awk "BEGIN{print ${end}-0.2}")
        filters+=",drawtext=fontfile='${FONT}':text='${text}':fontcolor=${AMBER_BRIGHT}:fontsize=${fontsize}:x=(w-text_w)/2:y=${y_expr}:enable='between(t,${start},${end})':alpha='if(lt(t,${fade_in}),(t-${start})*5,if(gt(t,${fade_out}),(${end}-t)*5,1))'"
    done
    "${FFMPEG}" -y -loglevel error \
        -framerate 24 -i "${act_dir}/f_%06d.png" \
        -vf "${filters}" \
        -frames:v "${ACT_FRAMES}" \
        -c:v libx264 -pix_fmt yuv420p -crf 18 -preset medium -r 24 \
        "${out_mp4}"
}

make_card() {
    local out_mp4="$1"
    local duration="$2"
    shift 2
    local captions=("$@")
    local filters="vignette=PI/2.6"
    for spec in "${captions[@]}"; do
        IFS='|' read -r text fontsize color y_expr start dur <<< "${spec}"
        local end fade_in fade_out
        end=$(awk "BEGIN{print ${start}+${dur}}")
        fade_in=$(awk "BEGIN{print ${start}+0.4}")
        fade_out=$(awk "BEGIN{print ${end}-0.4}")
        filters+=",drawtext=fontfile='${FONT}':text='${text}':fontcolor=${color}:fontsize=${fontsize}:x=(w-text_w)/2:y=${y_expr}:enable='between(t,${start},${end})':alpha='if(lt(t,${fade_in}),(t-${start})*2.5,if(gt(t,${fade_out}),(${end}-t)*2.5,1))'"
    done
    "${FFMPEG}" -y -loglevel error \
        -f lavfi -i "color=c=0x080400:s=1080x1920:r=24:d=${duration}" \
        -vf "${filters}" \
        -c:v libx264 -pix_fmt yuv420p -crf 18 -preset medium \
        "${out_mp4}"
}

make_audio() {
    local out_audio="$1"
    local duration="$2"
    "${FFMPEG}" -y -loglevel error \
        -f lavfi -i "sine=frequency=98:sample_rate=44100:duration=${duration}" \
        -f lavfi -i "sine=frequency=146.83:sample_rate=44100:duration=${duration}" \
        -f lavfi -i "sine=frequency=196:sample_rate=44100:duration=${duration}" \
        -f lavfi -i "anoisesrc=color=brown:duration=${duration}:sample_rate=44100" \
        -filter_complex "\
            [0:a]volume='0.42 + 0.06*sin(2*PI*0.07*t)':eval=frame[d1]; \
            [1:a]volume='0.27 + 0.05*sin(2*PI*0.05*t + 1.2)':eval=frame[d2]; \
            [2:a]volume='0.16 + 0.08*sin(2*PI*0.11*t + 2.4)':eval=frame[d3]; \
            [3:a]volume=0.06,lowpass=f=550,highpass=f=80[hiss]; \
            [d1][d2][d3][hiss]amix=inputs=4:duration=longest:normalize=0, \
            aecho=0.6:0.45:80|150:0.3|0.2, \
            afade=t=in:st=0:d=2.0, \
            afade=t=out:st=$(awk "BEGIN{print ${duration}-2.0}"):d=2.0, \
            loudnorm=I=-18:TP=-2.0:LRA=11" \
        -ac 2 -c:a aac -b:a 192k \
        "${out_audio}"
}

echo ">> rendering title card"
make_card "${OUT_DIR}/n_title.mp4" "${TITLE_DURATION}" \
    "OBSERVATIONS|96|${AMBER_BRIGHT}|h*0.46|0.2|2.5" \
    "04 — 06|44|${AMBER_MID}|h*0.54|0.5|2.2"

echo ">> rendering Act IV (DLA growth)"
make_act "render_frames/dla_grow" "${OUT_DIR}/n_act4.mp4" \
    "04.|0.8|2.5|72|h*0.40" \
    "a fractal that grew itself.|0.8|2.8|54|h*0.78" \
    "every walker found a place.|3.7|2.8|54|h*0.78" \
    "the place was always already there.|6.7|3.0|54|h*0.78"

echo ">> rendering Act V (Lorenz)"
make_act "render_frames/lorenz_grow" "${OUT_DIR}/n_act5.mp4" \
    "05.|0.8|2.5|72|h*0.40" \
    "three numbers learned to dance.|0.8|2.8|54|h*0.78" \
    "they never landed in the same place twice.|3.8|3.2|54|h*0.78" \
    "they never left the dance floor.|7.2|2.5|54|h*0.78"

echo ">> rendering Act VI (Particles)"
make_act "render_frames/parts_swirl" "${OUT_DIR}/n_act6.mp4" \
    "06.|0.8|2.5|72|h*0.40" \
    "we set them moving.|0.8|2.5|54|h*0.78" \
    "we did not set them anywhere in particular.|3.5|3.2|54|h*0.78" \
    "they found each other anyway.|6.9|2.6|54|h*0.78"

echo ">> rendering end card"
make_card "${OUT_DIR}/n_end.mp4" "${END_DURATION}" \
    "the archive grows.|56|${AMBER_BRIGHT}|h*0.45|0.3|2.2" \
    "claude × everythingsings.art|34|${AMBER_DIM}|h*0.55|0.7|2.0"

echo ">> stitching with crossfades"
"${FFMPEG}" -y -loglevel error \
    -i "${OUT_DIR}/n_title.mp4" \
    -i "${OUT_DIR}/n_act4.mp4" \
    -i "${OUT_DIR}/n_act5.mp4" \
    -i "${OUT_DIR}/n_act6.mp4" \
    -i "${OUT_DIR}/n_end.mp4" \
    -filter_complex "\
        [0][1]xfade=transition=fade:duration=${XFADE}:offset=2.5[v01]; \
        [v01][2]xfade=transition=fade:duration=${XFADE}:offset=12[v02]; \
        [v02][3]xfade=transition=fade:duration=${XFADE}:offset=21.5[v03]; \
        [v03][4]xfade=transition=fade:duration=${XFADE}:offset=31[vfinal]" \
    -map "[vfinal]" \
    -c:v libx264 -pix_fmt yuv420p -crf 18 -preset medium -movflags +faststart \
    "${OUT_DIR}/_obs04_silent.mp4"

echo ">> generating ambient audio"
make_audio "${OUT_DIR}/_obs04_ambient.m4a" "${TOTAL_DURATION}"

echo ">> muxing audio + video"
"${FFMPEG}" -y -loglevel error \
    -i "${OUT_DIR}/_obs04_silent.mp4" \
    -i "${OUT_DIR}/_obs04_ambient.m4a" \
    -c:v copy -c:a aac -b:a 192k -shortest -movflags +faststart \
    "${OUT_DIR}/observations_04_06.mp4"

echo ">> done: ${OUT_DIR}/observations_04_06.mp4"
