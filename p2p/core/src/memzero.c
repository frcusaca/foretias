#include "platform.h"
#include "foretias_core.h"

void foretias_memzero(void* ptr, size_t len) {
    volatile uint8_t* p = (volatile uint8_t*)ptr;
    while (len--) *p++ = 0;
}
