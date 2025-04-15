#pragma once
#pragma clang diagnostic ignored "-Wunknown-attributes"

#include "defs.h"

// Call to interpreter to log a string at the specified pointer with specified length
void bl_log(char *ptr, size_t num_bytes) __attribute__((__import_module__("blaulicht"), __import_name__("log"), ));

void bl_midi(uint8_t status, uint8_t data0, uint8_t data1) __attribute__((__import_module__("blaulicht"), __import_name__("midi"), ));

// void bl_reload() __attribute__((__import_module__("blaulicht"), __import_name__("reload"), ));

//
// Wasm imports used only by Reef C std code, not user code
//

// dataset imports
// size_t _reef_dataset_len() __attribute__((__import_module__("reef"), __import_name__("dataset_len"), ));
// void _reef_dataset_write(uint8_t *ptr) __attribute__((__import_module__("reef"), __import_name__("dataset_write"), ));

// result import
// void _reef_result(size_t result_type, uint8_t *ptr, size_t len)
//     __attribute__((__import_module__("reef"), __import_name__("result"), ));
