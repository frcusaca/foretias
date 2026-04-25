#include "platform.h"
#include "fortias_core.h"

void fortias_memzero(void* ptr, size_t len) {
    volatile uint8_t* p = (volatile uint8_t*)ptr;
    while (len--) *p++ = 0;
}
