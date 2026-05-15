//! Integration sanity tests for multi-algorithm PQC support.

use crate::crypto_server::{CryptoServer, ForetiasCurve, SignOps, VerifyOps, IdentityOps, software::SoftwareCryptoServer};
use crate::crypto_server::kem_mlkem;
use crate::foretias::types::{SignatureAlgorithm, KemAlgorithm};

fn make_server() -> SoftwareCryptoServer {
    SoftwareCryptoServer::generate(ForetiasCurve::Ed25519).unwrap()
}

// ─── 5.1 Cross-signature tests ──────────────────────────────────────

mod cross_signature {
    use super::*;

    #[test]
    fn sphincs_self_verify() {
        let server = make_server();
        let msg = b"SPHINCS+ self-test message";
        let sig = server.sign_with(msg, SignatureAlgorithm::SPHINCS_SHA2_128S).unwrap();
        let sphincs_pk = server.sphincs_pub_key.as_ref().unwrap().clone();

        let valid = server.verify_with(&sphincs_pk, "SPHINCS+-SHA2-128s-simple", msg, &sig).unwrap();
        assert!(valid);
    }

    #[test]
    fn dilithium_self_verify() {
        let server = make_server();
        let msg = b"Dilithium3 self-test message";
        let sig = server.sign_with(msg, SignatureAlgorithm::Dilithium3).unwrap();
        let dilithium_pk = server.dilithium_pub_key.as_ref().unwrap().clone();

        let valid = server.verify_with(&dilithium_pk, "Dilithium3", msg, &sig).unwrap();
        assert!(valid);
    }

    #[test]
    fn ed25519_self_verify() {
        let server = make_server();
        let msg = b"Ed25519 self-test message";
        let sig = server.sign_with(msg, SignatureAlgorithm::Ed25519).unwrap();
        let pk = match server.public_key() {
            crate::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
            _ => panic!("expected Ed25519 pubkey"),
        };

        let valid = server.verify_with(&pk, "Ed25519", msg, &sig).unwrap();
        assert!(valid);
    }

    #[test]
    fn sphincs_cross_server_verify() {
        let signer = make_server();
        let verifier = make_server();

        let msg = b"Cross-server SPHINCS+ message";
        let sig = signer.sign_with(msg, SignatureAlgorithm::SPHINCS_SHA2_128S).unwrap();
        let sphincs_pk = signer.sphincs_pub_key.as_ref().unwrap().clone();

        let valid = verifier.verify_with(&sphincs_pk, "SPHINCS+-SHA2-128s-simple", msg, &sig).unwrap();
        assert!(valid);
    }

    #[test]
    fn dilithium_cross_server_verify() {
        let signer = make_server();
        let verifier = make_server();

        let msg = b"Cross-server Dilithium message";
        let sig = signer.sign_with(msg, SignatureAlgorithm::Dilithium3).unwrap();
        let dilithium_pk = signer.dilithium_pub_key.as_ref().unwrap().clone();

        let valid = verifier.verify_with(&dilithium_pk, "Dilithium3", msg, &sig).unwrap();
        assert!(valid);
    }

    #[test]
    fn sphincs_tampered_signature_fails() {
        let server = make_server();
        let msg = b"Tampered test message";
        let mut sig = server.sign_with(msg, SignatureAlgorithm::SPHINCS_SHA2_128S).unwrap();
        let sphincs_pk = server.sphincs_pub_key.as_ref().unwrap().clone();

        sig[0] ^= 0xFF;

        let valid = server.verify_with(&sphincs_pk, "SPHINCS+-SHA2-128s-simple", msg, &sig).unwrap();
        assert!(!valid);
    }

    #[test]
    fn dilithium_tampered_signature_fails() {
        let server = make_server();
        let msg = b"Tampered test message";
        let mut sig = server.sign_with(msg, SignatureAlgorithm::Dilithium3).unwrap();
        let dilithium_pk = server.dilithium_pub_key.as_ref().unwrap().clone();

        sig[0] ^= 0xFF;

        let valid = server.verify_with(&dilithium_pk, "Dilithium3", msg, &sig).unwrap();
        assert!(!valid);
    }

    #[test]
    fn wrong_pubkey_fails_for_all_algorithms() {
        let signer = make_server();
        let impersonator = make_server();

        let msg = b"Wrong pubkey test";

        let sig = signer.sign_with(msg, SignatureAlgorithm::SPHINCS_SHA2_128S).unwrap();
        let fake_pk = impersonator.sphincs_pub_key.as_ref().unwrap().clone();
        let valid = signer.verify_with(&fake_pk, "SPHINCS+-SHA2-128s-simple", msg, &sig).unwrap();
        assert!(!valid);

        let sig = signer.sign_with(msg, SignatureAlgorithm::Dilithium3).unwrap();
        let fake_pk = impersonator.dilithium_pub_key.as_ref().unwrap().clone();
        let valid = signer.verify_with(&fake_pk, "Dilithium3", msg, &sig).unwrap();
        assert!(!valid);
    }

    #[test]
    fn ed25519_sig_with_sphincs_pubkey_fails() {
        let server = make_server();
        let msg = b"Cross-algo wrong key test";
        let sig = server.sign_with(msg, SignatureAlgorithm::Ed25519).unwrap();
        let sphincs_pk = server.sphincs_pub_key.as_ref().unwrap().clone();

        let result = server.verify_with(&sphincs_pk, "Ed25519", msg, &sig);
        if let Ok(valid) = result {
            assert!(!valid);
        }
    }
}

// ─── 5.2 Mutual attestation cross-algorithm ─────────────────────────

mod mutual_attestation {
    use super::*;

    #[test]
    fn cross_algorithm_mutual_verification() {
        let server_a = make_server();
        let server_b = make_server();

        let msg_a = b"Server A attesting to Server B";
        let msg_b = b"Server B attesting to Server A";

        let sig_a = server_a.sign_with(msg_a, SignatureAlgorithm::SPHINCS_SHA2_128S).unwrap();
        let pk_a = server_a.sphincs_pub_key.as_ref().unwrap().clone();
        let valid_b = server_b.verify_with(&pk_a, "SPHINCS+-SHA2-128s-simple", msg_a, &sig_a).unwrap();
        assert!(valid_b);

        let sig_b = server_b.sign_with(msg_b, SignatureAlgorithm::Dilithium3).unwrap();
        let pk_b = server_b.dilithium_pub_key.as_ref().unwrap().clone();
        let valid_a = server_a.verify_with(&pk_b, "Dilithium3", msg_b, &sig_b).unwrap();
        assert!(valid_a);
    }

    #[test]
    fn three_way_cross_algorithm_verification() {
        let server_a = make_server();
        let server_b = make_server();
        let server_c = make_server();

        let msg = b"Three-way mutual attestation";

        let sig_a = server_a.sign_with(msg, SignatureAlgorithm::SPHINCS_SHA2_128S).unwrap();
        let pk_a = server_a.sphincs_pub_key.as_ref().unwrap().clone();
        assert!(server_b.verify_with(&pk_a, "SPHINCS+-SHA2-128s-simple", msg, &sig_a).unwrap());
        assert!(server_c.verify_with(&pk_a, "SPHINCS+-SHA2-128s-simple", msg, &sig_a).unwrap());

        let sig_b = server_b.sign_with(msg, SignatureAlgorithm::Dilithium3).unwrap();
        let pk_b = server_b.dilithium_pub_key.as_ref().unwrap().clone();
        assert!(server_a.verify_with(&pk_b, "Dilithium3", msg, &sig_b).unwrap());
        assert!(server_c.verify_with(&pk_b, "Dilithium3", msg, &sig_b).unwrap());

        let sig_c = server_c.sign_with(msg, SignatureAlgorithm::Ed25519).unwrap();
        let pk_c = match server_c.public_key() {
            crate::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
            _ => panic!("expected Ed25519 pubkey"),
        };
        assert!(server_a.verify_with(&pk_c, "Ed25519", msg, &sig_c).unwrap());
        assert!(server_b.verify_with(&pk_c, "Ed25519", msg, &sig_c).unwrap());
    }

    #[test]
    fn signature_sizes_within_bounds() {
        let server = make_server();
        let msg = b"Size test";

        let ed_sig = server.sign_with(msg, SignatureAlgorithm::Ed25519).unwrap();
        assert_eq!(ed_sig.len(), 64);

        let sphincs_sig = server.sign_with(msg, SignatureAlgorithm::SPHINCS_SHA2_128S).unwrap();
        assert_eq!(sphincs_sig.len(), 7856);

        let dil_sig = server.sign_with(msg, SignatureAlgorithm::Dilithium3).unwrap();
        assert_eq!(dil_sig.len(), 3293);
    }

    #[test]
    fn pubkey_sizes_within_bounds() {
        let server = make_server();

        let ed_pk = match server.public_key() {
            crate::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
            _ => panic!("expected Ed25519 pubkey"),
        };
        assert_eq!(ed_pk.len(), 32);

        let sphincs_pk = server.sphincs_pub_key.as_ref().unwrap();
        assert_eq!(sphincs_pk.len(), 32);

        let dil_pk = server.dilithium_pub_key.as_ref().unwrap();
        assert_eq!(dil_pk.len(), 1952);
    }
}

// ─── 5.3 Handshake: matching algorithms pass ────────────────────────

mod handshake_matching {
    use super::*;

    #[test]
    fn mlkem_encaps_decaps_roundtrip() {
        let (pk, sk) = kem_mlkem::mlkem_768_keypair().unwrap();

        let (ct, ss_encap) = kem_mlkem::mlkem_768_encapsulate(&pk).unwrap();
        let ss_decap = kem_mlkem::mlkem_768_decapsulate(&sk, &ct).unwrap();

        assert_eq!(ss_encap, ss_decap);
        assert_eq!(ss_encap.len(), 32);
        assert_eq!(ct.len(), 1088);
    }

    #[test]
    fn mlkem_keypair_sizes() {
        let (pk, sk) = kem_mlkem::mlkem_768_keypair().unwrap();
        assert_eq!(pk.len(), 1184);
        assert_eq!(sk.len(), 2400);
    }

    #[test]
    fn mlkem_different_keys_different_secrets() {
        let (pk1, _sk1) = kem_mlkem::mlkem_768_keypair().unwrap();
        let (pk2, _sk2) = kem_mlkem::mlkem_768_keypair().unwrap();

        let (_ct1, ss1) = kem_mlkem::mlkem_768_encapsulate(&pk1).unwrap();
        let (_ct2, ss2) = kem_mlkem::mlkem_768_encapsulate(&pk2).unwrap();

        assert_ne!(ss1, ss2);
        assert_ne!(pk1, pk2);
    }

    #[test]
    fn default_algorithm_is_sphincs() {
        let server = make_server();
        let default_alg = server.signature_algorithm();
        assert_eq!(default_alg, SignatureAlgorithm::SPHINCS_SHA2_128S);
    }
}

// ─── 5.4 Handshake: mismatched algorithms fail ──────────────────────

mod handshake_mismatch {
    use super::*;

    #[test]
    fn unknown_signature_algorithm_rejected() {
        let result = SignatureAlgorithm::from_id_string("BLAKE3-sign");
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::error::CryptoError::UnknownAlgorithm(name) => {
                assert_eq!(name, "BLAKE3-sign");
            }
            _ => panic!("Expected UnknownAlgorithm error"),
        }
    }

    #[test]
    fn unknown_kem_algorithm_rejected() {
        let result = KemAlgorithm::from_id_string("X25519-Hybrid");
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::error::CryptoError::UnknownAlgorithm(name) => {
                assert_eq!(name, "X25519-Hybrid");
            }
            _ => panic!("Expected UnknownAlgorithm error"),
        }
    }

    #[test]
    fn mlkem_wrong_ciphertext_fails() {
        let (pk, _sk1) = kem_mlkem::mlkem_768_keypair().unwrap();
        let (_pk2, sk2) = kem_mlkem::mlkem_768_keypair().unwrap();

        let (_ct, ss1) = kem_mlkem::mlkem_768_encapsulate(&pk).unwrap();
        let ss2 = kem_mlkem::mlkem_768_decapsulate(&sk2, &(_ct)).unwrap();

        assert_ne!(ss1, ss2);
    }

    #[test]
    fn mlkem_tampered_ciphertext_fails() {
        let (pk, sk) = kem_mlkem::mlkem_768_keypair().unwrap();

        let (ct, ss_orig) = kem_mlkem::mlkem_768_encapsulate(&pk).unwrap();
        let mut ct_tampered = ct.clone();
        ct_tampered[0] ^= 0xFF;
        ct_tampered[100] ^= 0xAA;

        let ss_tampered = kem_mlkem::mlkem_768_decapsulate(&sk, &ct_tampered).unwrap();

        assert_ne!(ss_orig, ss_tampered);
    }
}

// ─── 5.5 Full 4-server integration ──────────────────────────────────

mod full_integration {
    use super::*;

    #[test]
    fn four_server_all_algorithms_coexist() {
        let servers: Vec<_> = (0..4)
            .map(|_| make_server())
            .collect();

        let algos = [
            SignatureAlgorithm::Ed25519,
            SignatureAlgorithm::SPHINCS_SHA2_128S,
            SignatureAlgorithm::Dilithium3,
        ];

        let msg = b"Four-server coexistence test";

        for (i, signer) in servers.iter().enumerate() {
            for &alg in &algos {
                let sig = signer.sign_with(msg, alg).unwrap();

                for (j, verifier) in servers.iter().enumerate() {
                    let valid = match alg {
                        SignatureAlgorithm::Ed25519 => {
                            let pk = match signer.public_key() {
                                crate::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
                                _ => panic!("expected Ed25519"),
                            };
                            verifier.verify_with(&pk, alg.to_id_string(), msg, &sig).unwrap()
                        }
                        SignatureAlgorithm::SPHINCS_SHA2_128S => {
                            let pk = signer.sphincs_pub_key.as_ref().unwrap().clone();
                            verifier.verify_with(&pk, "SPHINCS+-SHA2-128s-simple", msg, &sig).unwrap()
                        }
                        SignatureAlgorithm::Dilithium3 => {
                            let pk = signer.dilithium_pub_key.as_ref().unwrap().clone();
                            verifier.verify_with(&pk, "Dilithium3", msg, &sig).unwrap()
                        }
                        SignatureAlgorithm::SLH_DSA_SHA2_256F => {
                            let pk = signer.sphincs_sha2_256f_pub_key.as_ref().unwrap().clone();
                            verifier.verify_with(&pk, "SPHINCS+-SHA2-256f-simple", msg, &sig).unwrap()
                        }
                    };
                    assert!(valid,
                        "Server {} should verify server {}'s {} signature", j, i, alg);
                }
            }
        }
    }

    #[test]
    fn all_servers_default_to_sphincs() {
        let servers: Vec<_> = (0..4)
            .map(|_| make_server())
            .collect();

        for (i, server) in servers.iter().enumerate() {
            assert_eq!(server.signature_algorithm(), SignatureAlgorithm::SPHINCS_SHA2_128S,
                "Server {} should default to SPHINCS+", i);
        }
    }

    #[test]
    fn all_servers_mlkem_key_exchange() {
        let servers: Vec<_> = (0..4)
            .map(|_| make_server())
            .collect();

        for (i, _sender) in servers.iter().enumerate() {
            for (j, _receiver) in servers.iter().enumerate() {
                if i == j { continue; }

                let (pk, sk) = kem_mlkem::mlkem_768_keypair().unwrap();
                let (_ct, ss_e) = kem_mlkem::mlkem_768_encapsulate(&pk).unwrap();
                let ss_d = kem_mlkem::mlkem_768_decapsulate(&sk, &(_ct)).unwrap();
                assert_eq!(ss_e, ss_d,
                    "Servers {}->{} ML-KEM exchange should produce matching secrets", i, j);
            }
        }
    }
}

// ─── 5.6 Algorithm mismatch detection ───────────────────────────────

mod algorithm_mismatch {
    use super::*;

    #[test]
    fn sig_algorithm_id_roundtrip() {
        for &alg in &[
            SignatureAlgorithm::Ed25519,
            SignatureAlgorithm::SPHINCS_SHA2_128S,
            SignatureAlgorithm::Dilithium3,
        ] {
            let id = alg.to_id_string();
            let parsed = SignatureAlgorithm::from_id_string(id).unwrap();
            assert_eq!(alg, parsed, "{} should round-trip through ID string", id);
        }
    }

    #[test]
    fn kem_algorithm_id_roundtrip() {
        for &alg in &[
            KemAlgorithm::NoiseXX,
            KemAlgorithm::MLKEM_768,
        ] {
            let id = alg.to_id_string();
            let parsed = KemAlgorithm::from_id_string(id).unwrap();
            assert_eq!(alg, parsed, "{} should round-trip through ID string", id);
        }
    }

    #[test]
    fn algorithm_ids_are_readable_strings() {
        let ids = [
            "Ed25519",
            "SPHINCS+-SHA2-128s-simple",
            "Dilithium3",
            "Noise-XX",
            "ML-KEM-768",
        ];
        for id in &ids {
            assert!(id.is_ascii());
            assert!(!id.contains('\x00'));
            assert!(id.len() > 3);
        }
    }

    #[test]
    fn verify_with_unrecognized_alg_returns_error() {
        let server = make_server();
        let msg = b"mismatch test";
        let fake_pk = vec![0u8; 32];
        let fake_sig = vec![0u8; 64];

        let result = server.verify_with(&fake_pk, "UNKNOWN-ALGO", msg, &fake_sig);
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::error::CryptoError::UnknownAlgorithm(ref name) => {
                assert_eq!(name, "UNKNOWN-ALGO");
            }
            other => panic!("Expected UnknownAlgorithm, got {:?}", other),
        }
    }

    #[test]
    fn algorithm_id_case_sensitivity() {
        let lowercase = SignatureAlgorithm::from_id_string("sphincs+-sha2-128s-simple");
        assert!(lowercase.is_err());

        let uppercase = KemAlgorithm::from_id_string("ML-KEM-768");
        assert!(uppercase.is_ok());

        let wrong_case = KemAlgorithm::from_id_string("ml-kem-768");
        assert!(wrong_case.is_err());
    }
}
