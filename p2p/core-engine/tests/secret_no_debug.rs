/// HR-3 enforcement: bindings must not derive Debug for secret-holding types.
use foretias_core::core::bindings::*;
use static_assertions::assert_not_impl_any;

assert_not_impl_any!(ForetiasPrivKey32: std::fmt::Debug);
assert_not_impl_any!(ForetiasSecretKeyVar: std::fmt::Debug);
assert_not_impl_any!(ForetiasKemSecretKey: std::fmt::Debug);
assert_not_impl_any!(ForetiasTbidV1SecretKey: std::fmt::Debug);
assert_not_impl_any!(ForetiasNoiseState: std::fmt::Debug);
assert_not_impl_any!(ForetiasFrostRound1: std::fmt::Debug);
