/// HR-3 enforcement: bindings must not derive Debug for secret-holding types.
use foretias_core::core::bindings::*;
use foretias_core::noise::NoiseSession;
use static_assertions::assert_not_impl_any;

assert_not_impl_any!(ForetiasPrivKey32: std::fmt::Debug, Copy, Clone);
assert_not_impl_any!(ForetiasSecretKeyVar: std::fmt::Debug);
assert_not_impl_any!(ForetiasKemSecretKey: std::fmt::Debug);
assert_not_impl_any!(ForetiasTbidV1SecretKey: std::fmt::Debug);
assert_not_impl_any!(ForetiasNoiseState: std::fmt::Debug);
assert_not_impl_any!(ForetiasFrostRound1: std::fmt::Debug);

// g3-b: NoiseSession must NOT be Send. The wrapped C11 ForetiasNoiseState has
// mutable send/recv nonce counters; cross-thread access risks nonce reuse in
// ChaCha20-Poly1305. See p2p/core-engine/src/noise.rs for the full rationale.
assert_not_impl_any!(NoiseSession: Send);
