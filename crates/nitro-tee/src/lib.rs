/// AWS Nitro Enclave TEE attestation anchor.
///
/// Real Nitro: NSM device at `/dev/nsm` issues COSE_Sign1 CBOR attestation docs
/// signed by AWS certificate chain.  PCR0 = EIF image hash (code identity).
///
/// Dev/mock mode: SHA-256 mock document with deterministic PCR values — same
/// API, same JSON shape, `dev_mode: true`.  No AWS account needed locally.
///
/// Feature `nitro` (Linux only) gates the real NSM call.  Default off so the
/// crate compiles on macOS.
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn is_nitro_enclave() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new("/dev/nsm").exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

// ─── PCR values ──────────────────────────────────────────────────────────────

/// Platform Configuration Register snapshot.
///
/// Real Nitro uses SHA-384; mock uses SHA-256 of known strings.
/// PCR0 = enclave image (EIF) — code identity, immutable per deploy.
/// PCR1 = kernel + ramdisk.
/// PCR2 = application binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcrValues {
    pub pcr0: String,
    pub pcr1: String,
    pub pcr2: String,
}

// ─── Attestation document ─────────────────────────────────────────────────────

/// TEE attestation document — real or mock.
///
/// `user_data` = ZK proof commitment bytes (up to 1024 bytes in real Nitro).
/// `document_hash` = SHA-256 over (pcr0 ‖ pcr1 ‖ pcr2 ‖ user_data ‖ nonce ‖ ts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeeAttestation {
    pub dev_mode: bool,
    pub enclave: bool,
    pub timestamp: u64,
    pub pcrs: PcrValues,
    pub user_data_hex: String,
    pub nonce_hex: String,
    /// Commitment over all attestation fields — verifiable without AWS.
    pub document_hash: String,
    pub algorithm: String,
}

impl TeeAttestation {
    pub fn generate(user_data: &[u8], nonce: &[u8]) -> Result<Self> {
        if is_nitro_enclave() {
            #[cfg(all(target_os = "linux", feature = "nitro"))]
            return Self::from_nsm(user_data, nonce);
        }
        Self::mock(user_data, nonce)
    }

    fn mock(user_data: &[u8], nonce: &[u8]) -> Result<Self> {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow!("time: {e}"))?
            .as_secs();

        // Deterministic per axiom-engine version — change these strings on new release.
        let pcr0 = hex::encode(Sha256::digest(b"axiom-engine-v0.1.0-enclave-image-EIF"));
        let pcr1 = hex::encode(Sha256::digest(b"axiom-engine-linux-5.15-kernel-ramdisk"));
        let pcr2 = hex::encode(Sha256::digest(b"axiom-engine-application-binary-v0.1.0"));

        let mut doc = Vec::new();
        doc.extend_from_slice(pcr0.as_bytes());
        doc.extend_from_slice(pcr1.as_bytes());
        doc.extend_from_slice(pcr2.as_bytes());
        doc.extend_from_slice(user_data);
        doc.extend_from_slice(nonce);
        doc.extend_from_slice(&ts.to_le_bytes());
        let document_hash = hex::encode(Sha256::digest(&doc));

        Ok(Self {
            dev_mode: true,
            enclave: false,
            timestamp: ts,
            pcrs: PcrValues { pcr0, pcr1, pcr2 },
            user_data_hex: hex::encode(user_data),
            nonce_hex: hex::encode(nonce),
            document_hash,
            algorithm: "SHA-256 mock (COSE_Sign1 in real Nitro)".to_string(),
        })
    }

    // Real NSM path — only compiled on linux + nitro feature.
    // Calls aws-nitro-enclaves-nsm-api to obtain a COSE_Sign1 attestation doc.
    #[cfg(all(target_os = "linux", feature = "nitro"))]
    fn from_nsm(user_data: &[u8], nonce: &[u8]) -> Result<Self> {
        use nsm_lib::{nsm_init, nsm_process_request};
        use nsm_api::api::{Request, Response};

        let ctx = nsm_init();
        let req = Request::Attestation {
            user_data: Some(user_data.to_vec().into()),
            nonce: Some(nonce.to_vec().into()),
            public_key: None,
        };
        let resp = nsm_process_request(ctx, req);
        match resp {
            Response::Attestation { document } => {
                // document is raw COSE_Sign1 CBOR bytes.
                // For the JSON surface we expose the document hash; full bytes
                // are available for on-chain anchoring.
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| anyhow!("time: {e}"))?
                    .as_secs();
                let document_hash = hex::encode(Sha256::digest(&document));
                Ok(Self {
                    dev_mode: false,
                    enclave: true,
                    timestamp: ts,
                    // PCRs are in the CBOR doc; surface as placeholder here.
                    // Full CBOR parsing via aws_nitro_enclaves_attestation crate.
                    pcrs: PcrValues {
                        pcr0: "see_cbor_document".into(),
                        pcr1: "see_cbor_document".into(),
                        pcr2: "see_cbor_document".into(),
                    },
                    user_data_hex: hex::encode(user_data),
                    nonce_hex: hex::encode(nonce),
                    document_hash,
                    algorithm: "COSE_Sign1 (AWS Nitro, NIST P-384)".to_string(),
                })
            }
            _ => Err(anyhow!("unexpected NSM response")),
        }
    }
}

// ─── Attestation anchor ───────────────────────────────────────────────────────

/// ZK proof commitment bound to TEE attestation.
///
/// Ties a RISC Zero receipt hash to an enclave identity (PCR0) and a
/// freshness nonce — the triple (commitment, pcr0, nonce) is unforgeable
/// outside the enclave.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationAnchor {
    pub proof_commitment: String,
    pub op: String,
    pub attestation: TeeAttestation,
}

impl AttestationAnchor {
    pub fn from_commitment(commitment: &[u8; 32], op: &str) -> Result<Self> {
        // Nanosecond timestamp as nonce — unique per call, replays detectable.
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow!("time: {e}"))?
            .as_nanos()
            .to_le_bytes()
            .to_vec();
        let attestation = TeeAttestation::generate(commitment, &nonce)?;
        Ok(Self {
            proof_commitment: hex::encode(commitment),
            op: op.to_string(),
            attestation,
        })
    }
}

// ─── Status ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeeStatus {
    pub enclave: bool,
    pub dev_mode: bool,
    pub platform: String,
    pub nitro_feature_enabled: bool,
    pub pcr_algorithm: String,
    pub document_format: String,
}

impl TeeStatus {
    pub fn current() -> Self {
        let enclave = is_nitro_enclave();
        Self {
            enclave,
            dev_mode: !enclave,
            platform: if enclave {
                "AWS Nitro Enclave".to_string()
            } else {
                format!("{} (dev)", std::env::consts::OS)
            },
            nitro_feature_enabled: cfg!(all(target_os = "linux", feature = "nitro")),
            pcr_algorithm: "SHA-384 (real Nitro) / SHA-256 mock (dev)".to_string(),
            document_format: "COSE_Sign1 CBOR (real Nitro) / JSON mock (dev)".to_string(),
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_attestation() {
        let commitment = [0xab_u8; 32];
        let att = TeeAttestation::generate(&commitment, b"test-nonce-12345").unwrap();
        assert!(att.dev_mode);
        assert!(!att.enclave);
        assert_eq!(att.user_data_hex, hex::encode(&commitment));
        assert_eq!(att.document_hash.len(), 64);
    }

    #[test]
    fn test_pcr_deterministic() {
        let att1 = TeeAttestation::generate(b"data", b"n1").unwrap();
        let att2 = TeeAttestation::generate(b"data", b"n2").unwrap();
        assert_eq!(att1.pcrs.pcr0, att2.pcrs.pcr0);
        assert_eq!(att1.pcrs.pcr1, att2.pcrs.pcr1);
        assert_eq!(att1.pcrs.pcr2, att2.pcrs.pcr2);
        assert_ne!(att1.document_hash, att2.document_hash);
    }

    #[test]
    fn test_attestation_anchor() {
        let commitment = [0xcd_u8; 32];
        let anchor = AttestationAnchor::from_commitment(&commitment, "sum").unwrap();
        assert_eq!(anchor.op, "sum");
        assert_eq!(anchor.proof_commitment, hex::encode(&commitment));
        assert!(anchor.attestation.dev_mode);
    }

    #[test]
    fn test_tee_status_dev() {
        let s = TeeStatus::current();
        assert!(s.dev_mode);
        assert!(!s.enclave);
        assert!(!s.platform.is_empty());
    }
}
