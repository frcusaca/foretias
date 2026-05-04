#include "foretias_core.h"

ForetiasCoreVersion foretias_core_version(void) {
    ForetiasCoreVersion v;
    v.major      = FORETIAS_CORE_VERSION_MAJOR;
    v.minor      = FORETIAS_CORE_VERSION_MINOR;
    v.build_hash = "unknown";  /* overridden by CMake / build.rs at link time */
    return v;
}
