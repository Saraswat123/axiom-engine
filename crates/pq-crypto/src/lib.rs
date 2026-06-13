/// Post-quantum cryptography — NIST FIPS 203/204 (Aug 2024).
///
/// ML-KEM-768  (CRYSTALS-Kyber)     — key encapsulation, replaces X25519
/// ML-DSA-65   (CRYSTALS-Dilithium) — digital signatures, replaces Ed25519
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

// ML-DSA
use ml_dsa::{
    EncodedSignature, EncodedVerifyingKey, Generate, KeyInit as DsaKeyInit, Keypair, MlDsa65,
    Signature, SignatureEncoding, Signer, SigningKey, Verifier, VerifyingKey,
};

// ML-KEM
use ml_kem::{
    kem::{Decapsulate, Encapsulate, Kem},
    DecapsulationKey, EncapsulationKey, KeyExport as KemKeyExport, MlKem768, TryKeyInit,
};

// ─── ML-DSA Identity ──────────────────────────────────────────────────────────

/// ML-DSA-65 (Dilithium3) keypair — sign proof commitments and P2P peer messages.
/// Verifying key = 1952 bytes  |  Signature = 3309 bytes  |  Signing key seed = 32 bytes.
pub struct PqIdentity {
    signing_key: SigningKey<MlDsa65>,
}

impl PqIdentity {
    /// Generate fresh keypair using OS randomness.
    pub fn generate() -> Self {
        Self {
            signing_key: SigningKey::<MlDsa65>::generate(),
        }
    }

    /// Reconstruct from 32-byte seed exported by `seed_bytes()`.
    pub fn from_seed(seed_bytes: &[u8; 32]) -> Self {
        // KeyInit::new_from_slice accepts &[u8] and converts to Array<u8, U32> internally
        let sk = DsaKeyInit::new_from_slice(seed_bytes).expect("seed must be exactly 32 bytes");
        Self { signing_key: sk }
    }

    /// Export 32-byte seed for persistence.
    pub fn seed_bytes(&self) -> [u8; 32] {
        let seed = self.signing_key.to_seed();
        let mut out = [0u8; 32];
        out.copy_from_slice(seed.as_ref());
        out
    }

    /// Sign `msg`. Returns raw ML-DSA-65 signature (3309 bytes).
    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        let sig: Signature<MlDsa65> = Signer::sign(&self.signing_key, msg);
        let enc: EncodedSignature<MlDsa65> = sig.encode();
        let b: &[u8] = enc.as_ref();
        b.to_vec()
    }

    /// Serialized verifying (public) key bytes — share with peers for verification.
    pub fn public_key_bytes(&self) -> Vec<u8> {
        let vk: VerifyingKey<MlDsa65> = Keypair::verifying_key(&self.signing_key);
        let enc: EncodedVerifyingKey<MlDsa65> = vk.encode();
        let b: &[u8] = enc.as_ref();
        b.to_vec()
    }

    /// Verify a signature. `verifying_key_bytes` comes from `public_key_bytes()`.
    pub fn verify_signature(
        verifying_key_bytes: &[u8],
        msg: &[u8],
        sig_bytes: &[u8],
    ) -> Result<bool> {
        let vk_enc = EncodedVerifyingKey::<MlDsa65>::try_from(verifying_key_bytes)
            .map_err(|_| anyhow!("verifying key wrong length"))?;
        let vk = VerifyingKey::<MlDsa65>::decode(&vk_enc);

        let sig_enc = EncodedSignature::<MlDsa65>::try_from(sig_bytes)
            .map_err(|_| anyhow!("signature wrong length"))?;
        let sig = Signature::<MlDsa65>::decode(&sig_enc)
            .ok_or_else(|| anyhow!("invalid ML-DSA signature encoding"))?;

        Ok(Verifier::verify(&vk, msg, &sig).is_ok())
    }

    /// First 16 bytes of public key as hex — compact peer identifier.
    pub fn peer_id_hex(&self) -> String {
        hex::encode(&self.public_key_bytes()[..16])
    }
}

// ─── ML-KEM Session ──────────────────────────────────────────────────────────

/// ML-KEM-768 (Kyber-768) key encapsulation — establish shared secret with a peer.
/// Encapsulation key (public) = 1184 bytes  |  Ciphertext = 1088 bytes  |  Secret = 32 bytes.
pub struct PqKem {
    dk: DecapsulationKey<MlKem768>,
}

impl PqKem {
    /// Generate fresh KEM keypair. Returns `(PqKem, encapsulation_key_bytes)`.
    /// Broadcast the encapsulation key bytes so peers can send you an encrypted shared secret.
    pub fn generate() -> (Self, Vec<u8>) {
        let (dk, ek) = MlKem768::generate_keypair();
        // Use fully qualified path — ml_kem::KeyExport alias avoids clash with ml_dsa::KeyExport
        let ek_key = <EncapsulationKey<MlKem768> as KemKeyExport>::to_bytes(&ek);
        let b: &[u8] = ek_key.as_ref();
        (Self { dk }, b.to_vec())
    }

    /// Peer runs this with the encapsulation key you published.
    /// Returns `(ciphertext_to_send_back, shared_secret_32_bytes)`.
    pub fn encapsulate(ek_bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
        let ek: EncapsulationKey<MlKem768> =
            TryKeyInit::new_from_slice(ek_bytes).map_err(|e| anyhow!("ML-KEM ek decode: {e:?}"))?;
        let (ct, k) = ek.encapsulate();
        let ct_b: &[u8] = ct.as_ref();
        let k_b: &[u8] = k.as_ref();
        Ok((ct_b.to_vec(), k_b.to_vec()))
    }

    /// Holder of the decapsulation key recovers the shared secret from a ciphertext.
    pub fn decapsulate(&self, ct_bytes: &[u8]) -> Result<Vec<u8>> {
        let k = self
            .dk
            .decapsulate_slice(ct_bytes)
            .map_err(|e| anyhow!("ML-KEM decapsulate: {e:?}"))?;
        let b: &[u8] = k.as_ref();
        Ok(b.to_vec())
    }

    /// Serialize decapsulation key seed (64 bytes) for persistence.
    pub fn seed_bytes(&self) -> Vec<u8> {
        let seed = <DecapsulationKey<MlKem768> as KemKeyExport>::to_bytes(&self.dk);
        let b: &[u8] = seed.as_ref();
        b.to_vec()
    }
}

// ─── Signed Proof Commitment ──────────────────────────────────────────────────

/// ML-DSA-65 signed ZK proof commitment — broadcast over P2P gossipsub.
/// Receiver verifies commitment authenticity without trusting the sender.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedProof {
    /// 32-byte ZK proof commitment (risc0 receipt hash or AggregatedProof hash).
    pub commitment: [u8; 32],
    /// Computation that produced this proof (e.g. "sum", "dot", "norm").
    pub op: String,
    /// ML-DSA-65 signature over (commitment ++ op.as_bytes()).
    pub signature: Vec<u8>,
    /// Signer's ML-DSA-65 verifying key (1952 bytes).
    pub verifying_key: Vec<u8>,
    /// NIST standard label.
    pub algorithm: String,
}

impl SignedProof {
    /// Sign a proof commitment with this node's PQ identity.
    pub fn sign(commitment: [u8; 32], op: &str, identity: &PqIdentity) -> Self {
        let mut msg = Vec::with_capacity(32 + op.len());
        msg.extend_from_slice(&commitment);
        msg.extend_from_slice(op.as_bytes());
        Self {
            commitment,
            op: op.to_string(),
            signature: identity.sign(&msg),
            verifying_key: identity.public_key_bytes(),
            algorithm: "ML-DSA-65 (NIST FIPS 204)".to_string(),
        }
    }

    /// Verify the signature on this proof.
    pub fn verify(&self) -> Result<bool> {
        let mut msg = Vec::with_capacity(32 + self.op.len());
        msg.extend_from_slice(&self.commitment);
        msg.extend_from_slice(self.op.as_bytes());
        PqIdentity::verify_signature(&self.verifying_key, &msg, &self.signature)
    }

    pub fn commitment_hex(&self) -> String {
        hex::encode(self.commitment)
    }
}

// ─── Node PQ status (JSON-serialisable summary) ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PqStatus {
    pub enabled: bool,
    pub signature_algorithm: String,
    pub kem_algorithm: String,
    pub peer_id_prefix: String,
    pub vk_bytes: usize,
    pub sig_bytes: usize,
    pub kem_ct_bytes: usize,
    pub kem_ss_bytes: usize,
}

impl PqStatus {
    pub fn from_identity(id: &PqIdentity) -> Self {
        Self {
            enabled: true,
            signature_algorithm: "ML-DSA-65 (CRYSTALS-Dilithium, NIST FIPS 204)".to_string(),
            kem_algorithm: "ML-KEM-768 (CRYSTALS-Kyber, NIST FIPS 203)".to_string(),
            peer_id_prefix: id.peer_id_hex(),
            vk_bytes: 1952,
            sig_bytes: 3309,
            kem_ct_bytes: 1088,
            kem_ss_bytes: 32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dsa_sign_verify() {
        let id = PqIdentity::generate();
        let msg = b"axiom-engine proof commitment test";
        let sig = id.sign(msg);
        assert_eq!(sig.len(), 3309, "ML-DSA-65 signature must be 3309 bytes");
        assert!(PqIdentity::verify_signature(&id.public_key_bytes(), msg, &sig).unwrap());
    }

    #[test]
    fn test_dsa_wrong_key_fails() {
        let id1 = PqIdentity::generate();
        let id2 = PqIdentity::generate();
        let sig = id1.sign(b"message");
        // Verifying with wrong key should fail (not panic)
        let ok = PqIdentity::verify_signature(&id2.public_key_bytes(), b"message", &sig).unwrap();
        assert!(!ok, "wrong key must not verify");
    }

    #[test]
    fn test_kem_roundtrip() {
        let (kem, ek_bytes) = PqKem::generate();
        assert_eq!(ek_bytes.len(), 1184, "ML-KEM-768 ek must be 1184 bytes");
        let (ct, k_send) = PqKem::encapsulate(&ek_bytes).unwrap();
        assert_eq!(ct.len(), 1088, "ML-KEM-768 ciphertext must be 1088 bytes");
        assert_eq!(k_send.len(), 32, "shared secret must be 32 bytes");
        let k_recv = kem.decapsulate(&ct).unwrap();
        assert_eq!(k_send, k_recv, "encapsulate/decapsulate must agree");
    }

    #[test]
    fn test_signed_proof_roundtrip() {
        let id = PqIdentity::generate();
        let commitment = [0xab_u8; 32];
        let sp = SignedProof::sign(commitment, "sum", &id);
        assert!(sp.verify().unwrap(), "signed proof must verify");
        assert_eq!(sp.commitment_hex(), "ab".repeat(32));
    }

    #[test]
    fn test_seed_roundtrip() {
        let id = PqIdentity::generate();
        let seed = id.seed_bytes();
        let id2 = PqIdentity::from_seed(&seed);
        assert_eq!(id.public_key_bytes(), id2.public_key_bytes());
    }
}
