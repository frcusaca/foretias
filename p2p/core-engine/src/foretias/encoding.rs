//! Centralized serialization helpers for Foretias domain types.
//!
//! Provides newtype wrappers for byte arrays with built-in URL-safe base64
//! (no padding) JSON serialization, and convenience functions for JSON
//! encoding/decoding.

pub use self::ft_byte_array::FTByteArray;
pub use self::ft_byte_vector::FTByteVector;

pub fn to_json<T: serde::Serialize>(val: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(val)
}

pub fn to_json_pretty<T: serde::Serialize>(val: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(val)
}

pub fn from_json<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(s)
}

mod ft_byte_vector {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::ops::{Deref, DerefMut};

    /// Variable-length byte array with URL-safe base64 (no padding) JSON
    /// serialization.
    ///
    /// Serializes to a JSON *string* (not an integer array). Deserialization
    /// rejects integer arrays — only JSON strings are accepted.
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct FTByteVector {
        inner: Vec<u8>,
    }

    impl FTByteVector {
        pub fn new() -> Self {
            Self { inner: Vec::new() }
        }

        pub fn from_vec(inner: Vec<u8>) -> Self {
            Self { inner }
        }

        pub fn as_slice(&self) -> &[u8] {
            &self.inner
        }

        pub fn len(&self) -> usize {
            self.inner.len()
        }

        pub fn is_empty(&self) -> bool {
            self.inner.is_empty()
        }
    }

    impl From<Vec<u8>> for FTByteVector {
        fn from(inner: Vec<u8>) -> Self {
            Self { inner }
        }
    }

    impl From<FTByteVector> for Vec<u8> {
        fn from(val: FTByteVector) -> Self {
            val.inner
        }
    }

    impl From<&[u8]> for FTByteVector {
        fn from(slice: &[u8]) -> Self {
            Self {
                inner: slice.to_vec(),
            }
        }
    }

    impl From<&FTByteVector> for Vec<u8> {
        fn from(val: &FTByteVector) -> Self {
            val.inner.clone()
        }
    }

    impl Deref for FTByteVector {
        type Target = Vec<u8>;

        fn deref(&self) -> &Self::Target {
            &self.inner
        }
    }

    impl DerefMut for FTByteVector {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.inner
        }
    }

    impl Serialize for FTByteVector {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let encoded = URL_SAFE_NO_PAD.encode(&self.inner);
            serializer.serialize_str(&encoded)
        }
    }

    impl<'de> Deserialize<'de> for FTByteVector {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s = String::deserialize(deserializer)?;
            let bytes = URL_SAFE_NO_PAD
                .decode(&s)
                .map_err(serde::de::Error::custom)?;
            Ok(Self { inner: bytes })
        }
    }
}

mod ft_byte_array {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::fmt;
    use std::ops::Deref;

    /// Fixed-length byte array with URL-safe base64 (no padding) JSON
    /// serialization.
    ///
    /// Serializes to a JSON *string* (not an integer array). Deserialization
    /// rejects integer arrays and wrong-length byte sequences.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct FTByteArray<const N: usize> {
        inner: [u8; N],
    }

    impl<const N: usize> FTByteArray<N> {
        pub fn new(inner: [u8; N]) -> Self {
            Self { inner }
        }

        pub fn zeros() -> Self {
            Self { inner: [0u8; N] }
        }

        pub fn max() -> Self {
            Self { inner: [0xFFu8; N] }
        }

        pub fn as_slice(&self) -> &[u8] {
            &self.inner
        }
    }

    impl<const N: usize> From<[u8; N]> for FTByteArray<N> {
        fn from(inner: [u8; N]) -> Self {
            Self { inner }
        }
    }

    impl<const N: usize> From<FTByteArray<N>> for [u8; N] {
        fn from(val: FTByteArray<N>) -> Self {
            val.inner
        }
    }

    impl<const N: usize> From<&FTByteArray<N>> for [u8; N] {
        fn from(val: &FTByteArray<N>) -> Self {
            val.inner
        }
    }

    impl<const N: usize> Deref for FTByteArray<N> {
        type Target = [u8; N];

        fn deref(&self) -> &Self::Target {
            &self.inner
        }
    }

    impl<const N: usize> fmt::Display for FTByteArray<N> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", URL_SAFE_NO_PAD.encode(self.inner))
        }
    }

    impl<const N: usize> Serialize for FTByteArray<N> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let encoded = URL_SAFE_NO_PAD.encode(self.inner);
            serializer.serialize_str(&encoded)
        }
    }

    impl<'de, const N: usize> Deserialize<'de> for FTByteArray<N> {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s = String::deserialize(deserializer)?;
            let bytes = URL_SAFE_NO_PAD
                .decode(&s)
                .map_err(serde::de::Error::custom)?;
            if bytes.len() != N {
                return Err(serde::de::Error::custom(format_args!(
                    "expected {} bytes but got {} bytes",
                    N,
                    bytes.len()
                )));
            }
            let mut inner = [0u8; N];
            inner.copy_from_slice(&bytes);
            Ok(Self { inner })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_vector_roundtrip() {
        let v = FTByteVector::new();
        let json = to_json(&v).unwrap();
        assert_eq!(json, "\"\"");
        let decoded: FTByteVector = from_json(&json).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_single_byte_roundtrip() {
        let v = FTByteVector::from(vec![0xAB]);
        let json = to_json(&v).unwrap();
        let decoded: FTByteVector = from_json(&json).unwrap();
        assert_eq!(decoded.as_slice(), &[0xAB]);
    }

    #[test]
    fn test_all_zero_32_bytes() {
        let v = FTByteVector::from(vec![0u8; 32]);
        let json = to_json(&v).unwrap();
        let decoded: FTByteVector = from_json(&json).unwrap();
        assert_eq!(decoded.as_slice(), &[0u8; 32]);
    }

    #[test]
    fn test_all_0xff_64_bytes() {
        let v = FTByteVector::from(vec![0xFFu8; 64]);
        let json = to_json(&v).unwrap();
        let decoded: FTByteVector = from_json(&json).unwrap();
        assert_eq!(decoded.as_slice(), &[0xFFu8; 64]);
    }

    #[test]
    fn test_integer_array_rejection() {
        // Deserializing `[1,2,3]` (a JSON array) into FTByteVector must error
        let result: Result<FTByteVector, _> = from_json("[1, 2, 3]");
        assert!(result.is_err());
    }

    #[test]
    fn test_deref_transparency() {
        let mut v = FTByteVector::from(vec![1, 2, 3]);

        assert_eq!(v.len(), 3);
        assert_eq!(v[0], 1);

        v.push(4);
        assert_eq!(v.len(), 4);
        assert_eq!(v[3], 4);

        v.extend_from_slice(&[5, 6]);
        assert_eq!(v.as_slice(), &[1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_byte_array_16_roundtrip() {
        let arr: FTByteArray<16> = FTByteArray::new([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10,
        ]);
        let json = to_json(&arr).unwrap();
        let decoded: FTByteArray<16> = from_json(&json).unwrap();
        assert_eq!(
            decoded.as_slice(),
            &[
                0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
                0x0F, 0x10
            ]
        );
    }

    #[test]
    fn test_byte_array_32_roundtrip() {
        let arr: FTByteArray<32> = FTByteArray::new([0xABu8; 32]);
        let json = to_json(&arr).unwrap();
        let decoded: FTByteArray<32> = from_json(&json).unwrap();
        assert_eq!(decoded.as_slice(), &[0xABu8; 32]);
    }

    #[test]
    fn test_byte_array_96_roundtrip() {
        let arr: FTByteArray<96> = FTByteArray::new([0xCDu8; 96]);
        let json = to_json(&arr).unwrap();
        let decoded: FTByteArray<96> = from_json(&json).unwrap();
        assert_eq!(decoded.as_slice(), &[0xCDu8; 96]);
    }

    #[test]
    fn test_wrong_length_rejection() {
        // Encode 16 bytes, try to decode into FTByteArray<32> — must error
        let arr: FTByteArray<16> = FTByteArray::new([0x01u8; 16]);
        let json = to_json(&arr).unwrap();
        let result: Result<FTByteArray<32>, _> = from_json(&json);
        assert!(result.is_err());
    }
}
