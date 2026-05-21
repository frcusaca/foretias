#include "foretias_core.h"
void foretias_memzero(void* ptr, size_t len) {
    sodium_memzero(ptr, len);
}
