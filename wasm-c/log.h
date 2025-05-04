#pragma once
// #pragma clang diagnostic ignored "-Wunknown-attributes"

#include "defs.h"
#include "imports.h"


// Logs a NULL-terminated string
void bl_puts(char *message);
// Logs an integer
void bl_log_int(int val);

// Calculates the length of NULL-terminated string
unsigned long strlen(const char *ptr);
// Converts an integer into a string
char *itoa(int value, char *result, int base);
