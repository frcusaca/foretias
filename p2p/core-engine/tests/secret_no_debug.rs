/// HR-3 enforcement: bindings must not derive Debug for secret-holding types.
/// This test confirms (via a `static_assertions::assert_not_impl_any!`) that
/// `Debug` is not implemented for each enumerated FFI struct.
///
/// See HOW_SECRET_IS_SECURED_BY_SOFTWARE_SPEC.md REQ-Z2.3, REQ-Z8.4.

use foretias_core::core::bindings::*;
use static_assertions::assert_not_impl_any;

assert_not_impl_any!(ForetiasPrivKey32: std::fmt::Debug);
assert_not_impl_any!(ForetiasSecretKeyVar: std::fmt::Debug);
assert_not_impl_any!(ForetiasKemSecretKey: std::fmt::Debug);
assert_not_impl_any!(ForetiasTbidV1SecretKey: std::fmt::Debug);
assert_not_impl_any!(ForetiasNoiseState: std::fmt::Debug);
assert_not_impl_any!(ForetiasFrostRound1: std::fmt::Debug);
