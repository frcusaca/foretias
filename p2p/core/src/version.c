#include "fortias_core.h"

FortiasCoreVersion fortias_core_version(void) {
    FortiasCoreVersion v;
    v.major      = FORTIAS_CORE_VERSION_MAJOR;
    v.minor      = FORTIAS_CORE_VERSION_MINOR;
    v.build_hash = "unknown";  /* overridden by CMake / build.rs at link time */
    return v;
}
