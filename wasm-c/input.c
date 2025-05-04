#include "blaulicht.h"

/////////////////////////////
// Constants.
/////////////////////////////

// TODO: also add a modifier for this, best would be the weel above the speed modifier
// Then, each touch of a dial will use the dial's data source for the left indicator.
// therefore, the VJ must touch the wheel to peak.
#define STROBE_TIME 60
// TODO: WHAT THE FUCK

#define SLOW_STROBE_SPEED 100

// TODO: also modify
#define BPM_STROBE_DURATION_MILLIS 10
#define MAX_BRIGHTNESS_NON_STROBE 50

#define FOG_TIME 5000

/////////////////////////////
// END Constants.
/////////////////////////////

/////////////////////////////
// DDJ-400: BEGIN MIDI MAPPING.
/////////////////////////////

// Hue.
#define CROSSFADER_STATUS 182
#define CROSSFADER_KIND 31

// Strobe brightness.
#define LEFT_FADER_STATUS 176
#define LEFT_FADER_KIND 19

// Normal color brightness.
#define RIGHT_FADER_STATUS 177
#define RIGHT_FADER_KIND 19

// Toggle strobe.
#define CUE_LEFT_STATUS 144
#define CUE_LEFT_KIND 84

// Toggle normal color.
#define CUE_RIGHT_STATUS 145
#define CUE_RIGHT_KIND 84

// Trigger both CUE-L and CUE-R.
#define RELEASE_FX_STATUS 148
#define RELEASE_FX_KIND 71

// Strobe speed modifier.
#define LEFT_FILTER_STATUS 182
#define LEFT_FILTER_KIND 23

// SLOW Strobe speed modifier.
#define LEFT_LOW_FILTER_STATUS 176
#define LEFT_LOW_FILTER_KIND 15

// Toggle strobe-to-music.
#define PERFORMANCE_LEFT_0_0_STATUS 151
#define PERFORMANCE_LEFT_0_0_KIND 0

// Toggle strobe-constant-on.
#define PERFORMANCE_LEFT_1_0_STATUS 151
#define PERFORMANCE_LEFT_1_0_KIND 1

// Toggle primary strobe on.
#define PERFORMANCE_LEFT_0_1_STATUS 151
#define PERFORMANCE_LEFT_0_1_KIND 4

// Toggle secondary strobe on.
#define PERFORMANCE_LEFT_1_1_STATUS 151
#define PERFORMANCE_LEFT_1_1_KIND 5

// Toggle auto-activate-strobe strobe
#define PERFORMANCE_LEFT_2_1_STATUS 151
#define PERFORMANCE_LEFT_2_1_KIND 7

// Toggle color-to-music.
#define PERFORMANCE_RIGHT_0_0_STATUS 153
#define PERFORMANCE_RIGHT_0_0_KIND 0

// Set color-profile spectrum.
#define PERFORMANCE_RIGHT_1_0_STATUS 153
#define PERFORMANCE_RIGHT_1_0_KIND 1

// Set color-profile maker.
#define PERFORMANCE_RIGHT_2_0_STATUS 153
#define PERFORMANCE_RIGHT_2_0_KIND 2

// Set color animation continuous.
#define PERFORMANCE_RIGHT_0_1_STATUS 153
#define PERFORMANCE_RIGHT_0_1_KIND 4

// Set color animation osci.
#define PERFORMANCE_RIGHT_0_2_STATUS 153
#define PERFORMANCE_RIGHT_0_2_KIND 5

// Set color animation-to-beat.
// TODO: make this lamp flash when there is no beat?
// Default to non-beat mode when there is no beat.
#define ROUND_CUE_RIGHT_STATUS 145
#define ROUND_CUE_RIGHT_KIND 12

// Fog machine.
#define ROUND_PLAY_LEFT_STATUS 144
#define ROUND_PLAY_LEFT_KIND 11

// Toggle fog machine.
#define ROUND_CUE_LEFT_STATUS 144
#define ROUND_CUE_LEFT_KIND 12

// Normal hue animation speed.
#define RIGHT_FILTER_STATUS 182
#define RIGHT_FILTER_KIND 24

// Beat hue animation speed.
#define RIGHT_LOW_FILTER_STATUS 177
#define RIGHT_LOW_FILTER_KIND 15

// Hue animation step size.
#define RIGHT_MID_FILTER_STATUS 177
#define RIGHT_MID_FILTER_KIND 11

// Reload button.
#define RELOOP_LEFT_STATUS 144
#define RELOOP_LEFT_KIND 77

// Left speed
#define LEFT_SPEED_STATUS 176
#define LEFT_SPEED_KIND 0

/////////////////////////////
// END MIDI MAPPING.
/////////////////////////////

/////////////////////////////
// BEGIN DATA ARRAY DEFINITION.
/////////////////////////////

// System.
#define INIT_TIME_INDEX 100
#define INIT_COMPLETE_INDEX 101
#define CURRENT_TIME_INDEX 102

// Strobe.
#define IS_WHITE_VALUE_INDEX 202
#define WHITE_VALUE_INDEX 203
#define BPM_STROBE_LAST_TICK_INDEX 204
#define BPM_STROBE_LAST_TICK_HIGH_INDEX 205
#define BPM_STROBE_MODE_INDEX 206
#define STROBE_START_INDEX 207
#define IS_STROBE_INDEX 208
#define FLASH_BRIGHTNESS_KEY 209
#define STROBE_ACTIVE_INDEX 210
#define STROBE_SPEED_MULTIPLIER_KEY 211
#define STROBE_TO_MUSIC_INDEX 212
#define STROBE_BEGIN_TIME 213
#define STROBE_WAS_OFF 214
#define STROBE_ON 215
#define STROBE_WAS_ON 216
#define STROBE_ON_ACTIVE_BRI 217
#define SLOW_STROBE_TIME_INDEX 218
#define SLOW_STROBE_MODE_INDEX 219 // whether the slow strobe panel is on or off
#define SLOW_STROBE_INDEX 220
#define SLOW_STROBE_CYCLE 221
#define SLOW_STROBE_DURATION_MULTIPLIER 222
#define FOG_START_INDEX 223
#define FOG_ACTIVE_INDEX 224
#define FOG_AUTO_ACTIVATE_ON_INDEX 225

// TODO: make sure these do not fuck up the other indices.
#define SLOW_STROBE_ACTIVATION_TIME_START_INDEX 230
#define SLOW_STROBE_DEACTIVATION_TIME_START_INDEX 250

#define SLOW_STROBE_LAST_ACTIVATION 261

#define PRIMARY_STROBE_ON_INDEX 262
#define SECONDARY_STROBE_ON_INDEX 263

#define AUTO_ACTIVATE_STROBE_INDEX 264
#define LAST_BASS_TRIGGER_TIME_INDEX 265

#define MOVING_HEAD_OFFSET 266

// Color.
#define CROSSFADER_KEY 300
#define NORMAL_BRIGHTNESS_KEY 301
#define COLOR_ACTIVE_INDEX 302
#define COLOR_TO_MUSIC_INDEX 303
#define COLOR_MODE_INDEX 304
#define HUE_ANIMATE_MODE_INDEX 305
#define HUE_ANIMATE_SPEED_INDEX 306
#define LAST_HUE_ANIMATE_TICK_INDEX 307
#define BEAT_HUE_ANIMATE_SPEED_INDEX 308
#define BEAT_HUE_ANIMATE_STEP_INDEX 309
#define ANIMATE_TO_BEAT_ON_INDEX 310
#define BEAT_ANIMATE_LAST_TICK_INDEX 311
#define LAST_MIDI_WRITE_VOLUME_INDICATOR 312

/////////////////////////////
// END DATA ARRAY DEFINITION.
/////////////////////////////

int elapsed(int32_t time, int32_t *data, int32_t index) { return time - data[index]; }

/////////////////////////////
// STROBE.
/////////////////////////////

void write_last_bpm_flash(int32_t *data, int32_t time) { data[BPM_STROBE_LAST_TICK_INDEX] = time; }
void write_last_bpm_flash_high(int32_t *data, int32_t time) { data[BPM_STROBE_LAST_TICK_HIGH_INDEX] = time; }
int since_last_bpm_flash(int32_t *data, int32_t time) { return elapsed(time, data, BPM_STROBE_LAST_TICK_INDEX); }
int since_last_bpm_flash_high(int32_t *data, int32_t time) {
    return elapsed(time, data, BPM_STROBE_LAST_TICK_HIGH_INDEX);
}

void set_slow_strobe_dark(uint8_t *dmx, int32_t *data);

void toggle_right_performance_1_0(int32_t *data) {
    if (data[COLOR_MODE_INDEX] == 1) {
        bl_puts("RESET");
        data[COLOR_MODE_INDEX] = 0;
    } else {
        data[COLOR_MODE_INDEX] = 1;
    }

    bl_midi(PERFORMANCE_RIGHT_1_0_STATUS, PERFORMANCE_RIGHT_1_0_KIND, (data[COLOR_MODE_INDEX] == 1) * 127);
    bl_midi(PERFORMANCE_RIGHT_2_0_STATUS, PERFORMANCE_RIGHT_2_0_KIND, (data[COLOR_MODE_INDEX] == 2) * 127);
}

void set_fog_machine_auto_on(int32_t *data, bool enabled) {
    data[FOG_AUTO_ACTIVATE_ON_INDEX] = enabled;
    bl_midi(ROUND_CUE_LEFT_STATUS, ROUND_CUE_LEFT_KIND, data[FOG_AUTO_ACTIVATE_ON_INDEX] * 127);
    if (data[FOG_AUTO_ACTIVATE_ON_INDEX]) {
        bl_puts("[ENABLED] FOG MACHINE ON DROP");
    } else {
        bl_puts("[DISABLED] FOG MACHINE ON DROP");
    }
}

void toggle_fog_machine_auto_on(int32_t *data) { set_fog_machine_auto_on(data, !data[FOG_AUTO_ACTIVATE_ON_INDEX]); }

void set_fog_machine(int32_t *data, bool enabled, uint8_t *dmx) {
    if (!data[FOG_ACTIVE_INDEX]) {
        data[FOG_START_INDEX] = data[CURRENT_TIME_INDEX];
        data[FOG_ACTIVE_INDEX] = 1;
        dmx[200] = 255;
        bl_midi(ROUND_PLAY_LEFT_STATUS, ROUND_PLAY_LEFT_KIND, 127);
    } else {
        data[FOG_ACTIVE_INDEX] = 0;
        dmx[200] = 0;
        bl_midi(ROUND_PLAY_LEFT_STATUS, ROUND_PLAY_LEFT_KIND, 0);
    }
}

#define SI data[SLOW_STROBE_CYCLE]

const int PANEL_DMX_START_ADDRS_LEN = 8;
int PANEL_DMX_START_ADDRS[] = {1, 17, 33, 49, 81, 129, 65, 161};

void set_slow_strobe_dark(uint8_t *dmx, int32_t *data) {
    // if (!data[SECONDARY_STROBE_ON_INDEX]) {
    //     return;
    // }

    int on_time_for_light = (int)(50.0 * (bl_pow(2.0, data[SLOW_STROBE_DURATION_MULTIPLIER])));

    if (data[SLOW_STROBE_DURATION_MULTIPLIER] == 1) {
        on_time_for_light = 10;
    }

    for (int i = 0; i < PANEL_DMX_START_ADDRS_LEN; i++) {
        // Check if something is active.
        if (data[SLOW_STROBE_ACTIVATION_TIME_START_INDEX + i] != 0 &&
            data[CURRENT_TIME_INDEX] - data[SLOW_STROBE_ACTIVATION_TIME_START_INDEX + i] > on_time_for_light) {
            int start_chan = PANEL_DMX_START_ADDRS[i];
            dmx[start_chan] = 0;       // Set offline.
            dmx[start_chan + 1] = 255; // Color temperature cold.
            data[SLOW_STROBE_ACTIVATION_TIME_START_INDEX + i] = 0;
            data[SLOW_STROBE_DEACTIVATION_TIME_START_INDEX + i] = data[CURRENT_TIME_INDEX];
        }
    }
}

void slow_strobe(bool v, uint8_t *dmx, int32_t *data) {
    return;
    if (!data[SECONDARY_STROBE_ON_INDEX]) {
        return;
    }

    int panel_bri = data[FLASH_BRIGHTNESS_KEY];

    if (data[STROBE_ON] || data[STROBE_WAS_ON]) {
        // if (data[STROBE_ON]) {
        //     // bl_puts("Constant strobe on");
        // } else {
        //     // bl_puts("Constant strobe off");
        // }

        for (int i = 0; i < PANEL_DMX_START_ADDRS_LEN; i++) {
            int start_chan = PANEL_DMX_START_ADDRS[i];
            dmx[start_chan] = data[STROBE_ON] * panel_bri;
            dmx[start_chan + 1] = 255; // Color temperature cold.
        }

        if (data[STROBE_ON]) {
            data[STROBE_WAS_ON] = 1;
        } else {
            data[STROBE_WAS_ON] = 0;

            // for (int i = 0; i < PANEL_DMX_START_ADDRS_LEN; i ++) {
            //     data[SLOW_STROBE_ACTIVATION_TIME_START_INDEX + i] = data[CURRENT_TIME_INDEX];
            // }
        }

        return;
    }

    // TODO: supposed to be a fix for the gleichmaessige aktivierung.
    int off_time_for_light = (int)(50.0 * (bl_pow(2.0, data[SLOW_STROBE_DURATION_MULTIPLIER])));

    if (data[SLOW_STROBE_DURATION_MULTIPLIER] == 1) {
        off_time_for_light = SLOW_STROBE_SPEED;
    }

    // Time trigger.
    if (data[IS_WHITE_VALUE_INDEX] && (data[CURRENT_TIME_INDEX] - data[SLOW_STROBE_TIME_INDEX] > SLOW_STROBE_SPEED) &&
        data[CURRENT_TIME_INDEX] - data[SLOW_STROBE_LAST_ACTIVATION] >
            (off_time_for_light / PANEL_DMX_START_ADDRS_LEN)) {
        SI = (SI + 1) % PANEL_DMX_START_ADDRS_LEN; // Advance strobe light pos.

        if (data[SLOW_STROBE_ACTIVATION_TIME_START_INDEX + SI] == 0 &&
            data[CURRENT_TIME_INDEX] - data[SLOW_STROBE_DEACTIVATION_TIME_START_INDEX] >
                (off_time_for_light / PANEL_DMX_START_ADDRS_LEN)) {
            int start_chan = PANEL_DMX_START_ADDRS[SI];
            dmx[start_chan] = 1 * panel_bri;
            dmx[start_chan + 1] = 255; // Color temp.

            data[SLOW_STROBE_ACTIVATION_TIME_START_INDEX + SI] = data[CURRENT_TIME_INDEX];
            data[SLOW_STROBE_DEACTIVATION_TIME_START_INDEX + SI] = 0;
            data[SLOW_STROBE_LAST_ACTIVATION] = data[CURRENT_TIME_INDEX];
        }
    }
}

// Sets all strobe light values to v.
void set_white(bool v, uint8_t *dmx, int32_t *data) {
    int32_t brightness = data[FLASH_BRIGHTNESS_KEY];

    if (!data[PRIMARY_STROBE_ON_INDEX]) {
        brightness = 0;
    }

    // Normal strobe lights definition here.
    // dmx[100] = brightness * v;
    // dmx[101] = brightness * v;
    // dmx[102] = brightness * v;

    dmx[101] = brightness * v;

    // if (!v) {
    //     dmx[105] = (dmx[105] + 10) % 255;
    // }
    // dmx[101] = 0;
    // dmx[102] = 0;
    // dmx[101] = brightness * v;
    // dmx[102] = brightness * v;
}

void set_left_cue(int32_t *data, bool enabled) {
    data[STROBE_ACTIVE_INDEX] = enabled;
    int toggled = 0x7F * enabled;
    bl_midi(CUE_LEFT_STATUS, CUE_LEFT_KIND, toggled);
}

void toggle_left_cue(int32_t *data) { set_left_cue(data, !data[STROBE_ACTIVE_INDEX]); }

void set_right_cue(int32_t *data, bool enabled) {
    data[COLOR_ACTIVE_INDEX] = enabled;
    int toggled = 0x7F * enabled;
    bl_midi(CUE_RIGHT_STATUS, CUE_RIGHT_KIND, toggled);
}

void toggle_right_cue(int32_t *data) { set_right_cue(data, !data[COLOR_ACTIVE_INDEX]); }

void set_right_performace_0_0(int32_t *data, bool enabled) {
    data[COLOR_TO_MUSIC_INDEX] = enabled;
    int color = 0x7F * enabled;
    bl_midi(PERFORMANCE_RIGHT_0_0_STATUS, PERFORMANCE_RIGHT_0_0_KIND, color);
}

void update_left_perf_indicators(int32_t *data) {
    bl_midi(PERFORMANCE_LEFT_0_0_STATUS, PERFORMANCE_LEFT_0_0_KIND, 0x7F * data[STROBE_TO_MUSIC_INDEX]);
    bl_midi(PERFORMANCE_LEFT_1_0_STATUS, PERFORMANCE_LEFT_1_0_KIND, 0x7F * data[STROBE_ON]);
}

void set_left_performace_0_0(int32_t *data, uint8_t *dmx, bool enabled) {
    data[STROBE_TO_MUSIC_INDEX] = enabled;
    update_left_perf_indicators(data);
}

void set_left_performace_1_0(int32_t *data, bool enabled) {
    data[STROBE_ON] = enabled;
    update_left_perf_indicators(data);
}

void toggle_left_performance_0_0(int32_t *data, uint8_t *dmx) {
    set_left_performace_0_0(data, dmx, !data[STROBE_TO_MUSIC_INDEX]);
}
void toggle_left_performance_1_0(int32_t *data) { set_left_performace_1_0(data, !data[STROBE_ON]); }

void toggle_right_performance_0_0(int32_t *data) { set_right_performace_0_0(data, !data[COLOR_TO_MUSIC_INDEX]); }

void update_right_perf_indicators(int32_t *data) {
    bl_midi(PERFORMANCE_RIGHT_0_1_STATUS, PERFORMANCE_RIGHT_0_1_KIND, (data[HUE_ANIMATE_MODE_INDEX] == 1) * 127);
    bl_midi(PERFORMANCE_RIGHT_0_2_STATUS, PERFORMANCE_RIGHT_0_2_KIND, (data[HUE_ANIMATE_MODE_INDEX] == 2) * 127);

    bl_midi(PERFORMANCE_RIGHT_1_0_STATUS, PERFORMANCE_RIGHT_1_0_KIND, (data[COLOR_MODE_INDEX] == 1) * 127);
}

void set_left_performace_0_1(int32_t *data, bool enabled) {
    data[PRIMARY_STROBE_ON_INDEX] = enabled;
    bl_midi(PERFORMANCE_LEFT_0_1_STATUS, PERFORMANCE_LEFT_0_1_KIND, data[PRIMARY_STROBE_ON_INDEX] * 127);
    if (data[PRIMARY_STROBE_ON_INDEX]) {
        bl_puts("[ENABLED] Primary strobe");
    } else {
        bl_puts("[DISABLED] Primary strobe");
    }
}

void toggle_left_performance_0_1(int32_t *data) { set_left_performace_0_1(data, !data[PRIMARY_STROBE_ON_INDEX]); }

void set_left_performace_1_1(int32_t *data, bool enabled) {
    data[SECONDARY_STROBE_ON_INDEX] = enabled;
    bl_midi(PERFORMANCE_LEFT_1_1_STATUS, PERFORMANCE_LEFT_1_1_KIND, data[SECONDARY_STROBE_ON_INDEX] * 127);
    if (data[SECONDARY_STROBE_ON_INDEX]) {
        bl_puts("[ENABLED] Secondary strobe");
    } else {
        bl_puts("[DISABLED] Secondary strobe");
    }
}

void toggle_left_performance_1_1(int32_t *data) { set_left_performace_1_1(data, !data[SECONDARY_STROBE_ON_INDEX]); }

void set_left_performace_2_1(int32_t *data, bool enabled) {
    data[AUTO_ACTIVATE_STROBE_INDEX] = enabled;
    bl_midi(PERFORMANCE_LEFT_2_1_STATUS, PERFORMANCE_LEFT_2_1_KIND, data[AUTO_ACTIVATE_STROBE_INDEX] * 127);
    if (data[AUTO_ACTIVATE_STROBE_INDEX]) {
        bl_puts("[ENABLED] Automatic strobe driver");
    } else {
        bl_puts("[DISABLED] Automatic strobe driver");
    }
}

void toggle_left_performance_2_1(int32_t *data) { set_left_performace_2_1(data, !data[AUTO_ACTIVATE_STROBE_INDEX]); }

void set_right_performace_0_1(int32_t *data) {
    if (data[HUE_ANIMATE_MODE_INDEX] == 1) {
        data[HUE_ANIMATE_MODE_INDEX] = 0;
    } else {
        data[HUE_ANIMATE_MODE_INDEX] = 1;
    }

    update_right_perf_indicators(data);
}

void set_right_performace_1_1(int32_t *data) {
    if (data[HUE_ANIMATE_MODE_INDEX] == 2) {
        data[HUE_ANIMATE_MODE_INDEX] = 0;
    } else {
        data[HUE_ANIMATE_MODE_INDEX] = 2;
    }

    update_right_perf_indicators(data);
}

void toggle_right_performance_0_1(int32_t *data) { set_right_performace_0_1(data); }
void toggle_right_performance_1_1(int32_t *data) { set_right_performace_1_1(data); }

void set_right_cue_round(int32_t *data, bool enabled) {
    data[ANIMATE_TO_BEAT_ON_INDEX] = enabled;
    int color = 0x7F * enabled;
    bl_midi(ROUND_CUE_RIGHT_STATUS, ROUND_CUE_RIGHT_KIND, color);
}
void toggle_right_cue_round(int32_t *data) { set_right_cue_round(data, !data[ANIMATE_TO_BEAT_ON_INDEX]); }

void set_left_speed(int32_t *data, uint8_t midi_value) {
    data[MOVING_HEAD_OFFSET] = map(midi_value, 0, 127, 0, 255);
}

void set_left_vol_slider(int32_t *data, uint8_t midi_value) {
    data[FLASH_BRIGHTNESS_KEY] = map(midi_value, 0, 127, 0, 255);
}

void set_right_vol_slider(int32_t *data, uint8_t midi_value) {
    data[NORMAL_BRIGHTNESS_KEY] = map(midi_value, 0, 127, 0, 255);
}

void set_left_low_filter(int32_t *data, uint8_t midi_value) {
    int mapped = map(midi_value, 0, 127, 1, 5);
    data[SLOW_STROBE_DURATION_MULTIPLIER] = mapped;
    bl_midi(0xb0, 02, midi_value);
}

void set_left_filter(int32_t *data, uint8_t midi_value) {
    int mapped = map(midi_value, 0, 127, -4, 2);
    if (mapped == -3) {
        mapped = -2;
    }
    data[STROBE_SPEED_MULTIPLIER_KEY] = mapped;

    // BPM modifier indicators.
    int bpm_modifier = 0;
    if (data[STROBE_SPEED_MULTIPLIER_KEY] < 0) {
        bpm_modifier = map(abs(data[STROBE_SPEED_MULTIPLIER_KEY]), 1, 4, 60, 110);
    } else if (data[STROBE_SPEED_MULTIPLIER_KEY] > 0) {
        bpm_modifier = map(abs(data[STROBE_SPEED_MULTIPLIER_KEY]), 1, 2, 60, 70);
    } else if (data[STROBE_SPEED_MULTIPLIER_KEY] == 0) {
        bpm_modifier = map(1, 1, 2, 60, 70);
    }
    bl_midi(0xb0, 02, bpm_modifier);
}

void set_right_filter(int32_t *data, uint8_t midi_value) {
    int mapped = map(midi_value, 0, 127, 1, 255);
    data[HUE_ANIMATE_SPEED_INDEX] = mapped;
}

void set_right_low_filter(int32_t *data, uint8_t midi_value) {
    int mapped = map(midi_value, 0, 127, 1, 127);
    data[BEAT_HUE_ANIMATE_STEP_INDEX] = midi_value;
}

void set_right_mid_filter(int32_t *data, uint8_t midi_value) {
    int mapped = map(midi_value, 0, 127, -4, 4);
    if (mapped == -3) {
        mapped = -2;
    }
    data[BEAT_HUE_ANIMATE_SPEED_INDEX] = mapped;
}

void initialize(TickInput input, uint8_t *dmx_array, int32_t dmx_len, int32_t *data, int32_t data_len) {
    bl_puts("WASM: Initialized was called.");
    data[INIT_TIME_INDEX] = input.time;


    // Indicate reload begin.
    bl_midi(RELOOP_LEFT_STATUS, RELOOP_LEFT_KIND, 127);

    // Set faders.
    set_left_cue(data, 1);
    set_left_vol_slider(data, 127);
    set_right_cue(data, 1);
    set_right_vol_slider(data, 127);

    // Set filters.
    set_left_filter(data, 64);  // Maps to 0.
    set_right_filter(data, 64); // Maps to 0.

    // Set to-music-modes.
    set_left_performace_0_0(data, dmx_array, 1);
    set_right_performace_0_0(data, 1);

    set_right_low_filter(data, 64);
    set_right_mid_filter(data, 64);

    data[HUE_ANIMATE_MODE_INDEX] = 0;
    data[COLOR_MODE_INDEX] = 0;
    update_right_perf_indicators(data);

    // Default is BPM-mode.
    set_right_cue_round(data, 1);

    // Default is: fog machine activates on drop.
    set_fog_machine_auto_on(data, 1);

    // Default: all strobes on
    set_left_performace_0_1(data, 1);
    set_left_performace_1_1(data, 1);

    // Default: auto strobe on.
    set_left_performace_2_1(data, 1);

    data[LAST_MIDI_WRITE_VOLUME_INDICATOR] = 0;

    data[STROBE_BEGIN_TIME] = input.time;
    data[STROBE_WAS_OFF] = 1;

    //set_left_performace_0_0(data, dmx_array, 0);
    dmx_array[109] = 255;
    dmx_array[110] = 255;
    dmx_array[111] = 255;
    dmx_array[112] = 255;
    set_left_speed(data, 50);
}

void hue_advance_1(int32_t *data, int32_t progress_between_0_and_127) {
    data[CROSSFADER_KEY] = (data[CROSSFADER_KEY] + progress_between_0_and_127) % 127;
}

void hue_advance_2(int32_t *data) {
    if (data[CROSSFADER_KEY] == 127) {
        data[CROSSFADER_KEY] = 0;
    } else {
        data[CROSSFADER_KEY] = 127;
    }
}

void tick(TickInput input, uint8_t *dmx, int32_t dmx_len, int32_t *data, int32_t data_len, MidiEvent *midi,
          int32_t midi_len) {

    data[CURRENT_TIME_INDEX] = input.time;

    dmx[105] = 255 - data[MOVING_HEAD_OFFSET];
    dmx[105] = data[MOVING_HEAD_OFFSET];

    int init_delta = input.time - data[INIT_TIME_INDEX];
    if (init_delta > 1000 && init_delta < 2000 && !data[INIT_COMPLETE_INDEX]) {
        bl_midi(RELOOP_LEFT_STATUS, RELOOP_LEFT_KIND, 0);
        data[INIT_COMPLETE_INDEX] = 1;
    }

    // Volume indicators.
    if (data[ANIMATE_TO_BEAT_ON_INDEX]) {
        int bpm_modifier = 0;
        if (data[BEAT_HUE_ANIMATE_SPEED_INDEX] < 0) {
            bpm_modifier = map(abs(data[BEAT_HUE_ANIMATE_SPEED_INDEX]), 1, 4, 60, 110);
        } else if (data[BEAT_HUE_ANIMATE_SPEED_INDEX] > 0) {
            bpm_modifier = map(abs(data[BEAT_HUE_ANIMATE_SPEED_INDEX]), 1, 4, 60, 110);
        } else if (data[BEAT_HUE_ANIMATE_SPEED_INDEX] == 0) {
            bpm_modifier = map(1, 1, 2, 60, 70);
        }

        if (data[LAST_MIDI_WRITE_VOLUME_INDICATOR] != bpm_modifier) {
            bl_midi(0xb1, 02, bpm_modifier);
            data[LAST_MIDI_WRITE_VOLUME_INDICATOR] = bpm_modifier;
        }
    } else {
        if (data[LAST_MIDI_WRITE_VOLUME_INDICATOR] != input.volume) {
            int volume_value = map(input.volume, 0, 150, 0, 127);
            bl_midi(0xb1, 02, volume_value);
            data[LAST_MIDI_WRITE_VOLUME_INDICATOR] = input.volume;
        }
    }

    for (int i = 0; i < midi_len; i++) {
        if (midi[i].status == CROSSFADER_STATUS && midi[i].kind == CROSSFADER_KIND) {
            data[CROSSFADER_KEY] = midi[i].value;
        } else if (midi[i].status == LEFT_FADER_STATUS && midi[i].kind == LEFT_FADER_KIND) {
            set_left_vol_slider(data, midi[i].value);
        } else if (midi[i].status == RIGHT_FADER_STATUS && midi[i].kind == RIGHT_FADER_KIND) {
            set_right_vol_slider(data, midi[i].value);
        } else if (midi[i].status == LEFT_SPEED_STATUS && midi[i].kind == LEFT_SPEED_KIND) {
            set_left_speed(data, midi[i].value);
        } else if (midi[i].status == CUE_RIGHT_STATUS && midi[i].kind == CUE_RIGHT_KIND) {
            if (midi[i].value == 127) {
                toggle_right_cue(data);
            }
        } else if (midi[i].status == CUE_LEFT_STATUS && midi[i].kind == CUE_LEFT_KIND) {
            if (midi[i].value == 127) {
                toggle_left_cue(data);
            }
        } else if (midi[i].status == RELEASE_FX_STATUS && midi[i].kind == RELEASE_FX_KIND) {
            if (midi[i].value == 127) {
                toggle_left_cue(data);
                toggle_right_cue(data);
            }
        } else if (midi[i].status == LEFT_FILTER_STATUS && midi[i].kind == LEFT_FILTER_KIND) {
            set_left_filter(data, midi[i].value);
        } else if (midi[i].status == LEFT_LOW_FILTER_STATUS && midi[i].kind == LEFT_LOW_FILTER_KIND) {
            set_left_low_filter(data, midi[i].value);
        } else if (midi[i].status == RIGHT_FILTER_STATUS && midi[i].kind == RIGHT_FILTER_KIND) {
            set_right_filter(data, midi[i].value);
        } else if (midi[i].status == RIGHT_LOW_FILTER_STATUS && midi[i].kind == RIGHT_LOW_FILTER_KIND) {
            set_right_low_filter(data, midi[i].value);
        } else if (midi[i].status == RIGHT_MID_FILTER_STATUS && midi[i].kind == RIGHT_MID_FILTER_KIND) {
            set_right_mid_filter(data, midi[i].value);
        } else if (midi[i].status == PERFORMANCE_RIGHT_0_0_STATUS && midi[i].kind == PERFORMANCE_RIGHT_0_0_KIND) {
            if (midi[i].value == 127) {
                toggle_right_performance_0_0(data);
            }
        } else if (midi[i].status == PERFORMANCE_LEFT_0_0_STATUS && midi[i].kind == PERFORMANCE_LEFT_0_0_KIND) {
            if (midi[i].value == 127) {
                toggle_left_performance_0_0(data, dmx);
            }
        } else if (midi[i].status == PERFORMANCE_LEFT_1_0_STATUS && midi[i].kind == PERFORMANCE_LEFT_1_0_KIND) {
            if (midi[i].value == 127) {
                toggle_left_performance_1_0(data);
            }
        } else if (midi[i].status == ROUND_CUE_RIGHT_STATUS && midi[i].kind == ROUND_CUE_RIGHT_KIND) {
            if (midi[i].value == 127) {
                toggle_right_cue_round(data);
            }
        } else if (midi[i].status == PERFORMANCE_RIGHT_0_1_STATUS && midi[i].kind == PERFORMANCE_RIGHT_0_1_KIND) {
            if (midi[i].value == 127) {
                toggle_right_performance_0_1(data);
            }
        } else if (midi[i].status == PERFORMANCE_RIGHT_0_2_STATUS && midi[i].kind == PERFORMANCE_RIGHT_0_2_KIND) {
            if (midi[i].value == 127) {
                toggle_right_performance_1_1(data);
            }
        } else if (midi[i].status == PERFORMANCE_RIGHT_1_0_STATUS && midi[i].kind == PERFORMANCE_RIGHT_1_0_KIND) {
            if (midi[i].value == 127) {
                toggle_right_performance_1_0(data);
            }
        } else if (midi[i].status == PERFORMANCE_LEFT_0_1_STATUS && midi[i].kind == PERFORMANCE_LEFT_0_1_KIND) {
            if (midi[i].value == 127) {
                toggle_left_performance_0_1(data);
            }
        } else if (midi[i].status == PERFORMANCE_LEFT_1_1_STATUS && midi[i].kind == PERFORMANCE_LEFT_1_1_KIND) {
            if (midi[i].value == 127) {
                toggle_left_performance_1_1(data);
            }
        } else if (midi[i].status == PERFORMANCE_LEFT_2_1_STATUS && midi[i].kind == PERFORMANCE_LEFT_2_1_KIND) {
            if (midi[i].value == 127) {
                toggle_left_performance_2_1(data);
            }
        } else if (midi[i].status == ROUND_PLAY_LEFT_STATUS && midi[i].kind == ROUND_PLAY_LEFT_KIND) {
            if (midi[i].value == 127) {
                set_fog_machine(data, !data[FOG_ACTIVE_INDEX], dmx);
            }
        } else if (midi[i].status == ROUND_CUE_LEFT_STATUS && midi[i].kind == ROUND_CUE_LEFT_KIND) {
            if (midi[i].value == 127) {
                toggle_fog_machine_auto_on(data);
            }
        } else if (midi[i].status == PERFORMANCE_RIGHT_2_0_STATUS && midi[i].kind == PERFORMANCE_RIGHT_2_0_KIND) {
            if (midi[i].value == 127) {
                if (data[COLOR_MODE_INDEX] == 2) {
                    data[COLOR_MODE_INDEX] = 0;
                } else {
                    data[COLOR_MODE_INDEX] = 2;
                }

                bl_midi(PERFORMANCE_RIGHT_1_0_STATUS, PERFORMANCE_RIGHT_1_0_KIND, (data[COLOR_MODE_INDEX] == 1) * 127);
                bl_midi(PERFORMANCE_RIGHT_2_0_STATUS, PERFORMANCE_RIGHT_2_0_KIND, (data[COLOR_MODE_INDEX] == 2) * 127);
            }
            // TODO: toggle
            // toggleko
        } else {
            // ONLY LOG THIS WHEN A SPECIFIC MODE IS ENABLED.

            bl_puts("==========");
            bl_log_int(midi[i].status);
            bl_log_int(midi[i].kind);
            bl_log_int(midi[i].value);
        }
    }

    // Disable fog machine.
    if (input.time - data[FOG_START_INDEX] > FOG_TIME && data[FOG_ACTIVE_INDEX]) {
        set_fog_machine(data, 0, dmx);
    }

    // Crossfader code.
    if (data[COLOR_MODE_INDEX] == 0) {
        int hue = map(data[CROSSFADER_KEY], 0, 127, 0, 360);
        data[150] = hue;
    } else if (data[COLOR_MODE_INDEX] == 1) {
        int mapped = (data[CROSSFADER_KEY] > (127 / 2));

        if (mapped) {
            // CYAN
            data[150] = 180;
        } else {
            // MAGENTA
            data[150] = 300;
        }
    }

    // --- Rainbow RGB effect ---
    int hue = data[150]; // persistent hue
    int r, g, b;
    hsv_to_rgb(hue, &r, &g, &b);

    int volume_to_brightness = 0;
    if (since_last_bpm_flash(data, input.time) > 1000 || data[COLOR_ACTIVE_INDEX]) {
        int input_volume = input.volume;
        if (!data[COLOR_TO_MUSIC_INDEX]) {
            input_volume = 150;
        }
        volume_to_brightness = map(input_volume, 0, 150, 0, data[NORMAL_BRIGHTNESS_KEY]);
    }

    // Normal lights.

    dmx[21] = r;
    dmx[22] = g;
    dmx[23] = b;
    dmx[24] = volume_to_brightness;

    // TODO; diffusor

    // dmx[40] = volume_to_brightness;
    // dmx[41] = r;
    // dmx[42] = g;
    // dmx[43] = b;

    // dmx[250] = map(r, 0, 255, 0, volume_to_brightness);
    // dmx[251] = map(g, 0, 255, 0, volume_to_brightness);
    // dmx[252] = map(b, 0, 255, 0, volume_to_brightness);
    // dmx[253] = 0;
    // dmx[254] = 0;

    // dmx[260] = map(r, 0, 255, 0, volume_to_brightness);
    // dmx[261] = map(g, 0, 255, 0, volume_to_brightness);
    // dmx[262] = map(b, 0, 255, 0, volume_to_brightness);
    // dmx[263] = 0;
    // dmx[264] = 0;

    // dmx[100] = map(r, 0, 255, 0, volume_to_brightness);
    // dmx[101] = map(g, 0, 255, 0, volume_to_brightness);
    // dmx[102] = map(b, 0, 255, 0, volume_to_brightness);

    // --- Audio-reactive white strobe ---
    int time_since_strobe_start = elapsed(input.time, data, STROBE_START_INDEX);

    if (!data[STROBE_ON] && data[STROBE_ACTIVE_INDEX] &&
        ((input.bass_avg < 50 && input.bass > 100 && !data[IS_STROBE_INDEX]) ||
         (time_since_strobe_start < STROBE_TIME && data[IS_STROBE_INDEX]) && input.volume > 70)) {

        if (!data[IS_STROBE_INDEX]) {
            data[STROBE_START_INDEX] = input.time;
        }

        data[WHITE_VALUE_INDEX] = data[IS_WHITE_VALUE_INDEX] ? 0 : 255;
        data[IS_WHITE_VALUE_INDEX] = !data[IS_WHITE_VALUE_INDEX];

        set_white(data[IS_WHITE_VALUE_INDEX], dmx, data);
        bl_puts("Entered strobe branch.");
        write_last_bpm_flash(data, input.time);

        data[STROBE_BEGIN_TIME] = input.time;
        data[STROBE_WAS_OFF] = 0;

        if (!data[FOG_ACTIVE_INDEX] && data[FOG_AUTO_ACTIVATE_ON_INDEX]) {
            set_fog_machine(data, 1, dmx);
        }

        return;
    } else if (!data[STROBE_ON] && data[IS_WHITE_VALUE_INDEX]) {
        set_white(0, dmx, data);
        data[IS_WHITE_VALUE_INDEX] = 0;
        data[IS_STROBE_INDEX] = 0;
    }

    // --- BPM-based strobe ---
    int elapsed = since_last_bpm_flash(data, input.time);
    int bpm = input.bpm;

    float multiplier = 1.0;
    if (data[STROBE_SPEED_MULTIPLIER_KEY] < 0) {
        multiplier = (float)(-data[STROBE_SPEED_MULTIPLIER_KEY]);
    } else if (data[STROBE_SPEED_MULTIPLIER_KEY] > 0) {
        multiplier = 1.0 / ((float)data[STROBE_SPEED_MULTIPLIER_KEY]);
    }

    if (!data[STROBE_TO_MUSIC_INDEX] && bpm == 0) {
        bpm = 160;
    }

    int target_elapsed_bpm = (int)((1.0 / (float)bpm) * 60.0 * 1000.0 * multiplier);

    if (data[STROBE_ON]) {
        if (!data[IS_WHITE_VALUE_INDEX] || data[STROBE_ON_ACTIVE_BRI] != data[FLASH_BRIGHTNESS_KEY]) {
            data[IS_WHITE_VALUE_INDEX] = 1;
            data[WHITE_VALUE_INDEX] = 255;
            set_white(data[IS_WHITE_VALUE_INDEX], dmx, data);
            slow_strobe(data[IS_WHITE_VALUE_INDEX], dmx, data);
            data[STROBE_ON_ACTIVE_BRI] = data[FLASH_BRIGHTNESS_KEY];
        }
    } else if (data[STROBE_WAS_ON]) {
        slow_strobe(data[IS_WHITE_VALUE_INDEX], dmx, data);
    } else {
        if (input.bass_avg < 100 && input.bass < 100 && data[STROBE_TO_MUSIC_INDEX]) {
            if (data[IS_WHITE_VALUE_INDEX]) {
                bl_puts("Entered set-dark branch.");
                data[WHITE_VALUE_INDEX] = 0;
                data[IS_WHITE_VALUE_INDEX] = 0;
                set_white(data[IS_WHITE_VALUE_INDEX], dmx, data);
                slow_strobe(data[IS_WHITE_VALUE_INDEX], dmx, data);
                // set_slow_strobe_dark(dmx, data);
            }
        } else if (data[STROBE_ACTIVE_INDEX]) {
            // TODO: dont hardcode!!!
            // also only do this when the strobe is active!
            if (input.time - data[STROBE_BEGIN_TIME] > 5000 && !data[STROBE_WAS_OFF] &&
                data[AUTO_ACTIVATE_STROBE_INDEX] && data[STROBE_TO_MUSIC_INDEX] && input.bass_avg > 100) {
                bl_puts("AUTO deactivate strobe");
                set_left_cue(data, 0);
                data[STROBE_WAS_OFF] = 1;
            }

            if (elapsed > target_elapsed_bpm && !data[IS_WHITE_VALUE_INDEX]) {
                write_last_bpm_flash(data, input.time);
                data[IS_WHITE_VALUE_INDEX] = 1;
                data[WHITE_VALUE_INDEX] = 255;
                set_white(data[IS_WHITE_VALUE_INDEX], dmx, data);
                slow_strobe(data[IS_WHITE_VALUE_INDEX], dmx, data);
            } else if (data[IS_WHITE_VALUE_INDEX]) {
                write_last_bpm_flash_high(data, input.time);
                data[WHITE_VALUE_INDEX] = 0;
                data[IS_WHITE_VALUE_INDEX] = 0;
                set_white(data[IS_WHITE_VALUE_INDEX], dmx, data);
                slow_strobe(data[IS_WHITE_VALUE_INDEX], dmx, data);
            }
        }
    }

    if (input.bass_avg > 100) {
        data[LAST_BASS_TRIGGER_TIME_INDEX] = data[CURRENT_TIME_INDEX];
    }

    // Reactivate strobe if 5000 seconds since last bass.
    if (data[AUTO_ACTIVATE_STROBE_INDEX] && data[CURRENT_TIME_INDEX] - data[LAST_BASS_TRIGGER_TIME_INDEX] > 1000 &&
        !data[STROBE_ACTIVE_INDEX] && data[STROBE_WAS_OFF] && data[STROBE_TO_MUSIC_INDEX]) {
        bl_puts("AUTO Activate strobe.");
        set_left_cue(data, 1);
        data[STROBE_WAS_OFF] = 0;
    }

    set_slow_strobe_dark(dmx, data);

    // BPM-based animations.
    int bpm_animate_elapsed = input.time - data[LAST_HUE_ANIMATE_TICK_INDEX];

    // Advance HUE (slider pos) when using animation.
    int base_factor_animate = 10000;
    if (data[HUE_ANIMATE_MODE_INDEX] != 0) {
        if (data[ANIMATE_TO_BEAT_ON_INDEX]) {
            float multiplier = 1.0;
            if (data[BEAT_HUE_ANIMATE_SPEED_INDEX] < 0) {
                multiplier = (float)(-data[BEAT_HUE_ANIMATE_SPEED_INDEX]);
            } else if (data[BEAT_HUE_ANIMATE_SPEED_INDEX] > 0) {
                multiplier = 1.0 / ((float)data[BEAT_HUE_ANIMATE_SPEED_INDEX]);
            }

            int animate_target_elapsed_bpm = (int)((1.0 / (float)bpm) * 60.0 * 1000.0 * multiplier);

            if (data[ANIMATE_TO_BEAT_ON_INDEX] && bpm_animate_elapsed > animate_target_elapsed_bpm && bpm > 0) {
                data[LAST_HUE_ANIMATE_TICK_INDEX] = input.time;

                if (data[HUE_ANIMATE_MODE_INDEX] == 1) {
                    int advance_step = map(data[BEAT_HUE_ANIMATE_STEP_INDEX], 1, 127, 1, 50);
                    hue_advance_1(data, advance_step);
                } else if (data[HUE_ANIMATE_MODE_INDEX] == 2) {
                    hue_advance_2(data);
                }
            }
        } else if (data[CURRENT_TIME_INDEX] - data[LAST_HUE_ANIMATE_TICK_INDEX] >
                   ((int)(base_factor_animate / data[HUE_ANIMATE_SPEED_INDEX]))) {
            if (data[HUE_ANIMATE_MODE_INDEX] == 1) {
                int advance_step = map(data[BEAT_HUE_ANIMATE_STEP_INDEX], 1, 127, 1, 50);
                hue_advance_1(data, advance_step);
                hue_advance_1(data, advance_step);
            } else if (data[HUE_ANIMATE_MODE_INDEX] == 2) {
                hue_advance_2(data);
            }

            data[LAST_HUE_ANIMATE_TICK_INDEX] = data[CURRENT_TIME_INDEX];
        }
    }
}
