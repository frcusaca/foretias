#pragma once
/*
 * platform.h — C11 baseline, standard types, and static assertions.
 * Included by every .c file in the core library.
 */

#if __STDC_VERSION__ < 201112L
#error "C11 or later required"
#endif

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include <assert.h>
#include <string.h>

/* Internal-only headers for .c files */
#ifdef _WIN32
#include <windows.h>
#include <ntsecapi.h>
#else
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#endif

/* Compile-time sanity checks for types used throughout the core */
_Static_assert(sizeof(uint8_t)  == 1,  "uint8_t must be 1 byte");
_Static_assert(sizeof(uint32_t) == 4,  "uint32_t must be 4 bytes");
_Static_assert(sizeof(uint64_t) == 8,  "uint64_t must be 8 bytes");
_Static_assert(sizeof(size_t)  >= 4,  "size_t must be at least 4 bytes");
