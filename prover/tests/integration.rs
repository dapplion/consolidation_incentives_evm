//! Integration tests for the prover workspace

use proof_gen::{GindexCalculator, ProofGenerator};

#[test]
fn test_proof_generator_creates_correct_length_proofs() {
    let generator = ProofGenerator::new();

    let mut credentials = [0u8; 32];
    credentials[0] = 0x01;

    let bundle = generator
        .generate_proof(1000, 0, 42, 100, credentials)
        .expect("proof generation");

    assert_eq!(bundle.source_index, 42);
    assert_eq!(bundle.consolidation_index, 0);
    assert_eq!(bundle.activation_epoch, 100);

    // Verify proof lengths match expected
    let expected_consolidation_len = GindexCalculator::consolidation_proof_length() as usize;
    let expected_validator_len = GindexCalculator::validator_proof_length() as usize;

    assert_eq!(bundle.proof_consolidation.len(), expected_consolidation_len);
    assert_eq!(bundle.proof_credentials.len(), expected_validator_len);
    assert_eq!(bundle.proof_activation_epoch.len(), expected_validator_len);
}

#[test]
fn test_gindex_calculator_consistency() {
    // Verify gindex calculations are consistent across calls
    let gindex1 = GindexCalculator::consolidation_source_gindex(0);
    let gindex2 = GindexCalculator::consolidation_source_gindex(0);
    assert_eq!(gindex1, gindex2);

    // Different indices should give different gindices
    let gindex3 = GindexCalculator::consolidation_source_gindex(1);
    assert_ne!(gindex1, gindex3);
}
