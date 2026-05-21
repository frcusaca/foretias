#pragma once
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "../include/foretias_core.h"

static int g_pass = 0, g_fail = 0;

#define ASSERT_EQ(a, b, msg) do { \
    if ((a) == (b)) { g_pass++; printf("  PASS: %s\n", msg); } \
    else { g_fail++; printf("  FAIL: %s (%d != %d)\n", msg, (int)(a), (int)(b)); } \
} while(0)

#define ASSERT_PTR_NULL(p, msg) do { \
    if ((p) == NULL) { g_pass++; printf("  PASS: %s\n", msg); } \
    else { g_fail++; printf("  FAIL: %s (expected NULL)\n", msg); } \
} while(0)

#define ASSERT_PTR_NOT_NULL(p, msg) do { \
    if ((p) != NULL) { g_pass++; printf("  PASS: %s\n", msg); } \
    else { g_fail++; printf("  FAIL: %s (expected non-NULL)\n", msg); } \
} while(0)

#define ASSERT_NEQ(a, b, msg) do { \
    if ((a) != (b)) { g_pass++; printf("  PASS: %s\n", msg); } \
    else { g_fail++; printf("  FAIL: %s (%d == %d)\n", msg, (int)(a), (int)(b)); } \
} while(0)

#define TEST_REPORT(name) do { \
    printf("%s: %d passed, %d failed\n", name, g_pass, g_fail); \
} while(0)

static inline int test_suite_finish(void) {
    int f = g_fail;
    g_pass = 0;
    g_fail = 0;
    return f > 0 ? 1 : 0;
}
