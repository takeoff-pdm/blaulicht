#include "blaulicht.h"

// Internal imports
#include "imports.h"

// #include "input.c"

//
// Wasm entry point
//
// void blaulicht_main() {
//     size_t len = _reef_dataset_len();
//     size_t len_alloc = (len + 7) & ~0x07;

//     uint8_t *dataset_mem = malloc(len_alloc);
//     _reef_dataset_write(dataset_mem);

//     reef_result_int(0);

//     run(dataset_mem, len);
// }

// Result functions
// void reef_result_int(int value) {
//     uint8_t *ptr = (uint8_t *)&value;
//     _reef_result(0, ptr, 4);
// }
// void reef_result_bytes(uint8_t *ptr, size_t len) { _reef_result(1, ptr, len); }
// void reef_result_string(char *ptr, size_t len) { _reef_result(2, (uint8_t *)ptr, len); }

// Conversion functions
// uint32_t *from_little_endian(uint8_t *arr, size_t n_bytes) {
//     uint32_t *ret = (uint32_t *)malloc(n_bytes);

//     for (int i = 0; i < n_bytes / 4; i++) {
//         ret[i] = (arr[i * 4 + 3] << 24) + (arr[i * 4 + 2] << 16) + (arr[i * 4 + 1] << 8) + arr[i * 4];
//     }

//     return ret;
// }

// uint8_t *to_little_endian(uint32_t *arr, size_t n_bytes) {
//     uint8_t *ret = (uint8_t *)malloc(n_bytes);

//     for (int i = 0; i < n_bytes / 4; i++) {
//         ret[i * 4] = arr[i] & 0xff;
//         ret[i * 4 + 1] = (arr[i] >> 8) & 0xff;
//         ret[i * 4 + 2] = (arr[i] >> 16) & 0xff;
//         ret[i * 4 + 3] = (arr[i] >> 24) & 0xff;
//     }

//     return ret;
// }

// uint32_t *from_big_endian(uint8_t *arr, size_t n_bytes) {
//     uint32_t *ret = (uint32_t *)malloc(n_bytes);

//     for (int i = 0; i < n_bytes / 4; i++) {
//         ret[i] = (arr[i * 4] << 24) + (arr[i * 4 + 1] << 16) + (arr[i * 4 + 2] << 8) + arr[i * 4 + 3];
//     }

//     return ret;
// }

// uint8_t *to_big_endian(uint32_t *arr, size_t n_bytes) {
//     uint8_t *ret = (uint8_t *)malloc(n_bytes);

//     for (int i = 0; i < n_bytes / 4; i++) {
//         ret[i * 4 + 3] = arr[i] & 0xff;
//         ret[i * 4 + 2] = (arr[i] >> 8) & 0xff;
//         ret[i * 4 + 1] = (arr[i] >> 16) & 0xff;
//         ret[i * 4] = (arr[i] >> 24) & 0xff;
//     }

//     return ret;
// }

MidiEvent * decode_midi(uint32_t *midi, int32_t midi_len) {
    MidiEvent * return_value = malloc(sizeof(MidiEvent) * midi_len);

    for (int32_t i = 0; i < midi_len; i++) {
        uint8_t data1 = (uint8_t) (midi[i] & 0x000000FF);
        uint8_t data0 = (uint8_t) ((midi[i] & 0x0000FF00) >> 8);
        uint8_t status = (uint8_t) ((midi[i] & 0x00FF0000) >> 16);

        return_value[i].status = status;
        return_value[i].kind = data0;
        return_value[i].value = data1;
    }

    return return_value;
}

TickInput tickinput_from_array(int * tick_input_array, int tick_array_len) {
    #define ARRAY_LEN 7

    if (tick_array_len != ARRAY_LEN) {
        bl_puts("tick array len in 'tickinput_from_array' is not expected length:");
        bl_log_int(tick_array_len);
        abort();
    }

    TickInput input = {
        .time = tick_input_array[0],
        .volume = tick_input_array[1],
        .beat_volume = tick_input_array[2],
        .bass = tick_input_array[3],
        .bass_avg = tick_input_array[4],
        .bpm = tick_input_array[5],
        .initial = tick_input_array[6],
    };

    return input;
}

void internal_tick(
    int32_t * tick_input_array, int32_t tick_array_len,
    uint8_t * dmx_array, int32_t dmx_array_len,
    int32_t * data_array, int32_t data_len,
    uint32_t * midi_array, int32_t midi_len
) {
    TickInput input = tickinput_from_array(tick_input_array, tick_array_len);

    #define DMX_LEN 513

    if (dmx_array_len != DMX_LEN) {
        bl_puts("DMX array in 'internal_tick()' is not of exected length; got:");
        bl_log_int(dmx_array_len);
        bl_puts("expected: ");
        bl_log_int(DMX_LEN);
        abort();
    }

    if (input.initial) {
        initialize(input, dmx_array, dmx_array_len, data_array, data_len);
    } else {
        MidiEvent * decoded_midi = decode_midi(midi_array, midi_len);
        tick(input, dmx_array, dmx_array_len, data_array, data_len, decoded_midi, midi_len);
    }
}

void abort() {
    bl_puts("called abort()");
    while (1) {
    }
}

int32_t map(int32_t x, int32_t in_min, int32_t in_max, int32_t out_min, int32_t out_max) {
    return (x - in_min) * (out_max - out_min) / (in_max - in_min) + out_min;
}

double fmod(double a, double b) {
    return a - b * (int)(a / b);
}

void hsv_to_rgb(int32_t h, int32_t *r, int32_t *g, int32_t *b) {
    // Normalize the hue to range [0, 360)
    if (h < 0) h += 360;  // Ensure non-negative hue
    h = h % 360;           // Ensure hue is within the range of 0-360 degrees
    
    float s = 1.0f;  // Full saturation
    float v = 1.0f;  // Full brightness
    float c = v * s; // Chroma
    float x = c * (1 - fabs(fmod(h / 60.0f, 2.0f) - 1)); // Linear interpolation
    float m = v - c; // Match value to full brightness
    
    float rf = 0, gf = 0, bf = 0;

    // Linear calculation for the RGB components based on the hue angle
    if (h >= 0 && h < 60) {
        rf = c; gf = x; bf = 0;
    } else if (h >= 60 && h < 120) {
        rf = x; gf = c; bf = 0;
    } else if (h >= 120 && h < 180) {
        rf = 0; gf = c; bf = x;
    } else if (h >= 180 && h < 240) {
        rf = 0; gf = x; bf = c;
    } else if (h >= 240 && h < 300) {
        rf = x; gf = 0; bf = c;
    } else {
        rf = c; gf = 0; bf = x;
    }

    // Add the match value to each of the RGB components to shift them
    *r = (int32_t)((rf + m) * 255);
    *g = (int32_t)((gf + m) * 255);
    *b = (int32_t)((bf + m) * 255);
}


// Convert HSV hue (0–360) to RGB
void old_hsv_to_rgb(int32_t h, int32_t *r, int32_t *g, int32_t *b) {
    float s = 1.0f;
    float v = 1.0f;
    float c = v * s;
    float x = c * (1 - abs((h / 60 % 2) - 1));
    float m = v - c;

    float rf = 0, gf = 0, bf = 0;

    if (h < 60) {
        rf = c; gf = x; bf = 0;
    } else if (h < 120) {
        rf = x; gf = c; bf = 0;
    } else if (h < 180) {
        rf = 0; gf = c; bf = x;
    } else if (h < 240) {
        rf = 0; gf = x; bf = c;
    } else if (h < 300) {
        rf = x; gf = 0; bf = c;
    } else {
        rf = c; gf = 0; bf = x;
    }

    *r = (int32_t)((rf + m) * 255);
    *g = (int32_t)((gf + m) * 255);
    *b = (int32_t)((bf + m) * 255);
}

int32_t abs(int32_t x) {
    return (x < 0) ? -x : x;
}

double fabs(double x) {
    return (x < 0) ? -x : x;
}

double bl_pow(double base, int exponent) {
    double result = 1.0;
    int exp = exponent;

    if (exp < 0) {
        base = 1.0 / base;
        exp = -exp;
    }

    while (exp) {
        if (exp % 2 == 1) {
            result *= base;
        }
        base *= base;
        exp /= 2;
    }

    return result;
}