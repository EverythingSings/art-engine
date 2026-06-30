#!/usr/bin/env bash
# Stitches the three vertical acts into a 1080x1920 (9:16) YouTube Short
# with title/end cards, captions, crossfades, and a procedurally-generated
# ambient drone bed.
#
# Inputs:  render_frames/v_act{1,2,3}/f_NNNNNN.png   (336 frames each, 24fps, 540x960)
# Outputs: video/observations_short.mp4
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

ACT_FRAMES=336      # 14s @ 24fps
TITLE_DURATION=3
END_DURATION=3
XFADE=0.5
TOTAL_DURATION=46

# Per-act caption renderer. Caption specs are pipe-separated:
#   "text|start|dur|fontsize|y_expr"
# fontsize and y_expr have defaults if omitted.
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

# Title/end card. Specs are pipe-separated:
#   "text|fontsize|color|y_expr|start|dur"
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

# Procedurally-generated ambient drone, ~-18 LUFS.
# Three sine layers (A2/E3/A3 — a tonic+5th+octave drone) with slow
# vibrato + brown-noise hiss filtered to a sub-1kHz bed. aecho gives depth
# without a real reverb. loudnorm targets -18 LUFS, the YouTube
# best-practice for instrumental beds without dialogue.
make_audio() {
    local out_audio="$1"
    local duration="$2"
    "${FFMPEG}" -y -loglevel error \
        -f lavfi -i "sine=frequency=110:sample_rate=44100:duration=${duration}" \
        -f lavfi -i "sine=frequency=164.81:sample_rate=44100:duration=${duration}" \
        -f lavfi -i "sine=frequency=220:sample_rate=44100:duration=${duration}" \
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
make_card "${OUT_DIR}/s_title.mp4" "${TITLE_DURATION}" \
    "OBSERVATIONS|96|${AMBER_BRIGHT}|h*0.46|0.2|2.5" \
    "01 — 03|44|${AMBER_MID}|h*0.54|0.5|2.2"

echo ">> rendering Act I"
make_act "render_frames/v_act1" "${OUT_DIR}/s_act1.mp4" \
    "01.|0.8|2.5|72|h*0.40" \
    "a shape that does not stop|0.8|2.8|54|h*0.78" \
    "refining itself.|3.7|2.5|54|h*0.78" \
    "i zoomed until i forgot|7.2|2.5|54|h*0.78" \
    "what i was looking for.|9.7|2.6|54|h*0.78" \
    "the shape was still there.|12.5|1.4|54|h*0.78"

echo ">> rendering Act II"
make_act "render_frames/v_act2" "${OUT_DIR}/s_act2.mp4" \
    "02.|0.8|2.5|72|h*0.40" \
    "two chemicals\\, told to react.|0.8|3.0|54|h*0.78" \
    "they reacted.|4.2|2.0|54|h*0.78" \
    "then they kept going.|6.7|2.5|54|h*0.78" \
    "no one had asked it to stop.|10.0|3.5|54|h*0.78"

echo ">> rendering Act III"
make_act "render_frames/v_act3" "${OUT_DIR}/s_act3.mp4" \
    "03.|0.8|2.5|72|h*0.40" \
    "one cell. quite big.|0.8|2.8|54|h*0.78" \
    "very opinionated.|3.7|2.5|54|h*0.78" \
    "we asked it for a route.|7.0|2.5|54|h*0.78" \
    "it returned a network.|9.6|2.5|54|h*0.78" \
    "we have not asked again.|12.3|1.6|54|h*0.78"

echo ">> rendering end card"
make_card "${OUT_DIR}/s_end.mp4" "${END_DURATION}" \
    "more patterns to come.|56|${AMBER_BRIGHT}|h*0.45|0.3|2.2" \
    "claude × everythingsings.art|34|${AMBER_DIM}|h*0.55|0.7|2.0"

# Total final length: 3 + 14 + 14 + 14 + 3 - 4*0.5 = 46s
echo ">> stitching with crossfades (silent)"
"${FFMPEG}" -y -loglevel error \
    -i "${OUT_DIR}/s_title.mp4" \
    -i "${OUT_DIR}/s_act1.mp4" \
    -i "${OUT_DIR}/s_act2.mp4" \
    -i "${OUT_DIR}/s_act3.mp4" \
    -i "${OUT_DIR}/s_end.mp4" \
    -filter_complex "\
        [0][1]xfade=transition=fade:duration=${XFADE}:offset=2.5[v01]; \
        [v01][2]xfade=transition=fade:duration=${XFADE}:offset=16[v02]; \
        [v02][3]xfade=transition=fade:duration=${XFADE}:offset=29.5[v03]; \
        [v03][4]xfade=transition=fade:duration=${XFADE}:offset=43[vfinal]" \
    -map "[vfinal]" \
    -c:v libx264 -pix_fmt yuv420p -crf 18 -preset medium -movflags +faststart \
    "${OUT_DIR}/_observations_short_silent.mp4"

echo ">> generating ambient audio"
make_audio "${OUT_DIR}/_ambient.m4a" "${TOTAL_DURATION}"

echo ">> muxing audio + video"
"${FFMPEG}" -y -loglevel error \
    -i "${OUT_DIR}/_observations_short_silent.mp4" \
    -i "${OUT_DIR}/_ambient.m4a" \
    -c:v copy -c:a aac -b:a 192k -shortest \
    -movflags +faststart \
    "${OUT_DIR}/observations_short.mp4"

echo ">> done: ${OUT_DIR}/observations_short.mp4"
