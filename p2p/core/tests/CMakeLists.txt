# ── Foretias Core Tests ──────────────────────────────────────────────────

# Test sources (all compiled into one executable)
set(TEST_SOURCES
    test_all.c
    test_memzero.c
    test_rng.c
    test_version.c
    test_ed25519.c
    test_sha256.c
    test_merkle.c
    test_nullifier.c
    test_p256_stub.c
    test_blake3_stub.c
    test_noise.c
    test_frost_stub.c
    test_legacy_hash.c
    test_privkey.c
)

# Build test executable
add_executable(test_all ${TEST_SOURCES})
target_link_libraries(test_all PRIVATE foretias_core)
target_include_directories(test_all PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})

# Register with CTest
add_test(NAME foretias_core_tests COMMAND test_all)
