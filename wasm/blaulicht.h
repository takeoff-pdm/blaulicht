#pragma once

#include "defs.h"
#include "log.h"
#include "walloc.h"
#include "memory.h"
#include "imports.h"

// Wasm Function Imports.
// void reef_progress(float done) __attribute__((__import_module__("reef"), __import_name__("progress"), ));
// void reef_sleep(float seconds) __attribute__((__import_module__("reef"), __import_name__("sleep"), ));

// User main function definition
// void run(uint8_t *dataset, size_t len);

typedef struct {
    uint64_t time;
    uint8_t volume;
    uint8_t beat_volume;
    uint8_t bass;
    uint8_t bass_avg;
    uint8_t bpm;
    bool initial;
} TickInput;

typedef struct {
    uint8_t status;
    uint8_t kind;
    uint8_t value;
} MidiEvent;

void internal_tick(
    int32_t * tick_input_array, int32_t tick_array_len,
    uint8_t * dmx_array, int32_t dmx_array_len,
    int32_t * data_array, int32_t data_len,
    uint32_t * midi_array, int32_t midi_len
);

void initialize(TickInput input, uint8_t *dmx_array, int32_t dmx_array_len, int32_t * data, int32_t data_len);

void tick(
    TickInput input,
     uint8_t * dmx_array, int32_t dmx_array_len,
     int32_t *data, int32_t data_len,
     MidiEvent *midi, int32_t midi_len
);

// Result functions
// void reef_result_int(int value);
// void reef_result_bytes(uint8_t *ptr, size_t len);
// void reef_result_string(char *ptr, size_t len);

// Conversion functions
// uint32_t *from_little_endian(uint8_t *arr, size_t n_bytes);
// uint8_t *to_little_endian(uint32_t *arr, size_t n_bytes);
// uint32_t *from_big_endian(uint8_t *arr, size_t n_bytes);
// uint8_t *to_big_endian(uint32_t *arr, size_t n_bytes);

void abort();

int32_t map(int32_t x, int32_t in_min, int32_t in_max, int32_t out_min, int32_t out_max);

void hsv_to_rgb(int32_t h, int32_t *r, int32_t *g, int32_t *b);

int32_t abs(int32_t x);
double fabs(double x);
double bl_pow(double base, int exponent);