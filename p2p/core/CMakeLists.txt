cmake_minimum_required(VERSION 3.16)
project(fortias_core C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS OFF)

# ── Compiler flags ──────────────────────────────────────────────────────
set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -Wall -Wextra -Wpedantic -Werror")
set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -fno-strict-aliasing")
set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -fstack-protector")
set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -fvisibility=hidden")
set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -O2")

# ── Find libsodium ──────────────────────────────────────────────────────
find_library(SODIUM_LIBRARY NAMES sodium REQUIRED)
find_path(SODIUM_INCLUDE_DIR sodium.h REQUIRED)

# ── Find OpenSSL (for legacy MD5/SHA-1) ─────────────────────────────────
find_package(OpenSSL REQUIRED COMPONENTS Crypto)

# ── Source files ────────────────────────────────────────────────────────
set(FORTIAS_CORE_SOURCES
    src/version.c
    src/memzero.c
    src/rng_mix.c
    src/identity_ed25519.c
    src/identity_p256.c
    src/signing_ed25519.c
    src/signing_p256.c
    src/hash_sha256.c
    src/hash_blake3.c
    src/hash_legacy_insecure_md5.c
    src/hash_legacy_insecure_sha1.c
    src/noise_xx.c
    src/merkle.c
    src/frost_ed25519.c
    src/nullifier.c
    src/privkey.c
)

# ── Static library ──────────────────────────────────────────────────────
add_library(fortias_core STATIC ${FORTIAS_CORE_SOURCES})

target_include_directories(fortias_core
    PUBLIC  ${CMAKE_CURRENT_SOURCE_DIR}/include
    PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/src
)
target_include_directories(fortias_core SYSTEM PRIVATE ${SODIUM_INCLUDE_DIR})
target_include_directories(fortias_core SYSTEM PRIVATE ${OPENSSL_INCLUDE_DIR})
target_link_libraries(fortias_core PUBLIC ${SODIUM_LIBRARY} OpenSSL::Crypto)

# ── Install ──────────────────────────────────────────────────────────────
install(TARGETS fortias_core
    ARCHIVE DESTINATION lib
    LIBRARY DESTINATION lib
)
install(DIRECTORY include/ DESTINATION include)

# ── Tests ───────────────────────────────────────────────────────────────
enable_testing()
add_subdirectory(tests)
