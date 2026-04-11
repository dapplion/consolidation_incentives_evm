#![allow(clippy::too_many_arguments)]

use alloy::{
    hex::FromHex,
    primitives::{Address, FixedBytes},
    sol,
    sol_types::SolCall,
};
use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use serde::Deserialize;
use std::{fmt::Write as _, fs, path::PathBuf};

sol! {
    #[sol(rpc)]
    contract ConsolidationIncentives {
        function claimReward(
            uint64 beaconTimestamp,
            uint64 consolidationIndex,
            uint64 sourceIndex,
            uint64 activationEpoch,
            bytes32 sourceCredentials,
            bytes32[] calldata proofConsolidation,
            bytes32[] calldata proofCredentials,
            bytes32[] calldata proofActivationEpoch
        ) external;
    }

    #[sol(rpc)]
    contract MockBeaconRootsOracle {
        function setRoot(uint256 timestamp, bytes32 root) external;
    }
}

#[derive(Parser, Debug)]
#[command(name = "devnet-plan")]
#[command(about = "Build local-devnet claim commands from generated test vectors")]
struct Args {
    /// Path to generated JSON test vectors
    #[arg(long, default_value = "../../contracts/test-vectors/test_vectors.json")]
    vectors: PathBuf,

    /// Which valid claim to use (0-based index into `claims`)
    #[arg(long, default_value_t = 0)]
    claim_index: usize,

    /// Contract address for claim submission
    #[arg(long, default_value = "0x1111111111111111111111111111111111111111")]
    contract: Address,

    /// Oracle address for setRoot() (defaults to the EIP-4788 address)
    #[arg(long, default_value = "0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02")]
    oracle: Address,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Deserialize)]
struct TestVectorFile {
    block_root: String,
    beacon_timestamp: u64,
    max_epoch: u64,
    claims: Vec<TestClaim>,
}

#[derive(Debug, Deserialize)]
struct TestClaim {
    consolidation_index: u64,
    source_index: u64,
    activation_epoch: u64,
    source_credentials: String,
    proof_consolidation: Vec<String>,
    proof_credentials: Vec<String>,
    proof_activation_epoch: Vec<String>,
    expected_recipient: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DevnetPlan {
    claim_index: usize,
    beacon_timestamp: u64,
    max_epoch: u64,
    block_root: String,
    source_index: u64,
    consolidation_index: u64,
    activation_epoch: u64,
    expected_recipient: String,
    oracle_address: String,
    contract_address: String,
    set_root_calldata: String,
    claim_calldata: String,
    cast_set_root: String,
    cast_claim: String,
}

fn parse_bytes32(hex_str: &str) -> Result<FixedBytes<32>> {
    let bytes = <[u8; 32]>::from_hex(hex_str.trim_start_matches("0x"))
        .with_context(|| format!("invalid bytes32 hex: {hex_str}"))?;
    Ok(FixedBytes::from(bytes))
}

fn parse_bytes32_vec(values: &[String]) -> Result<Vec<FixedBytes<32>>> {
    values.iter().map(|value| parse_bytes32(value)).collect()
}

fn build_plan(
    vectors: TestVectorFile,
    claim_index: usize,
    oracle: Address,
    contract: Address,
) -> Result<DevnetPlan> {
    let claim = vectors
        .claims
        .get(claim_index)
        .with_context(|| format!("claim index {claim_index} out of range"))?;

    let block_root = parse_bytes32(&vectors.block_root)?;
    let source_credentials = parse_bytes32(&claim.source_credentials)?;
    let proof_consolidation = parse_bytes32_vec(&claim.proof_consolidation)?;
    let proof_credentials = parse_bytes32_vec(&claim.proof_credentials)?;
    let proof_activation_epoch = parse_bytes32_vec(&claim.proof_activation_epoch)?;

    let set_root = MockBeaconRootsOracle::setRootCall {
        timestamp: alloy::primitives::U256::from(vectors.beacon_timestamp),
        root: block_root,
    }
    .abi_encode();

    let claim_call = ConsolidationIncentives::claimRewardCall {
        beaconTimestamp: vectors.beacon_timestamp,
        consolidationIndex: claim.consolidation_index,
        sourceIndex: claim.source_index,
        activationEpoch: claim.activation_epoch,
        sourceCredentials: source_credentials,
        proofConsolidation: proof_consolidation,
        proofCredentials: proof_credentials,
        proofActivationEpoch: proof_activation_epoch,
    }
    .abi_encode();

    let block_root_hex = vectors.block_root.clone();

    Ok(DevnetPlan {
        claim_index,
        beacon_timestamp: vectors.beacon_timestamp,
        max_epoch: vectors.max_epoch,
        block_root: block_root_hex,
        source_index: claim.source_index,
        consolidation_index: claim.consolidation_index,
        activation_epoch: claim.activation_epoch,
        expected_recipient: claim.expected_recipient.clone(),
        oracle_address: oracle.to_string(),
        contract_address: contract.to_string(),
        set_root_calldata: format!("0x{}", hex::encode(&set_root)),
        claim_calldata: format!("0x{}", hex::encode(&claim_call)),
        cast_set_root: format!(
            "cast send {oracle} 'setRoot(uint256,bytes32)' {} {} --private-key $PRIVATE_KEY",
            vectors.beacon_timestamp, vectors.block_root
        ),
        cast_claim: format_cast_claim(contract, vectors.beacon_timestamp, claim),
    })
}

fn format_cast_claim(contract: Address, beacon_timestamp: u64, claim: &TestClaim) -> String {
    let mut command = format!(
        "cast send {contract} 'claimReward(uint64,uint64,uint64,uint64,bytes32,bytes32[],bytes32[],bytes32[])' {} {} {} {} {} ",
        beacon_timestamp,
        claim.consolidation_index,
        claim.source_index,
        claim.activation_epoch,
        claim.source_credentials
    );

    append_array(&mut command, &claim.proof_consolidation);
    command.push(' ');
    append_array(&mut command, &claim.proof_credentials);
    command.push(' ');
    append_array(&mut command, &claim.proof_activation_epoch);
    command.push_str(" --private-key $PRIVATE_KEY");
    command
}

fn append_array(out: &mut String, values: &[String]) {
    out.push('[');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(value);
    }
    out.push(']');
}

fn render_text(plan: &DevnetPlan) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Devnet validation plan");
    let _ = writeln!(out, "======================");
    let _ = writeln!(out, "claim index:          {}", plan.claim_index);
    let _ = writeln!(out, "beacon timestamp:     {}", plan.beacon_timestamp);
    let _ = writeln!(out, "max epoch:            {}", plan.max_epoch);
    let _ = writeln!(out, "source index:         {}", plan.source_index);
    let _ = writeln!(out, "consolidation index:  {}", plan.consolidation_index);
    let _ = writeln!(out, "activation epoch:     {}", plan.activation_epoch);
    let _ = writeln!(out, "expected recipient:   {}", plan.expected_recipient);
    let _ = writeln!(out, "oracle address:       {}", plan.oracle_address);
    let _ = writeln!(out, "contract address:     {}", plan.contract_address);
    let _ = writeln!(out, "block root:           {}", plan.block_root);
    let _ = writeln!(out);
    let _ = writeln!(out, "1. Seed the mock/root oracle");
    let _ = writeln!(out, "   {}", plan.cast_set_root);
    let _ = writeln!(out);
    let _ = writeln!(out, "2. Submit the reward claim");
    let _ = writeln!(out, "   {}", plan.cast_claim);
    let _ = writeln!(out);
    let _ = writeln!(out, "Raw calldata");
    let _ = writeln!(out, "-----------");
    let _ = writeln!(out, "setRoot:    {}", plan.set_root_calldata);
    let _ = writeln!(out, "claimReward:{}", plan.claim_calldata);
    out
}

fn main() -> Result<()> {
    let args = Args::parse();
    let raw = fs::read_to_string(&args.vectors)
        .with_context(|| format!("failed to read {}", args.vectors.display()))?;
    let vectors: TestVectorFile =
        serde_json::from_str(&raw).context("failed to parse test vector JSON")?;
    let plan = build_plan(vectors, args.claim_index, args.oracle, args.contract)?;

    match args.format {
        OutputFormat::Text => print!("{}", render_text(&plan)),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&plan)?),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_vectors() -> TestVectorFile {
        TestVectorFile {
            block_root: "0x1111111111111111111111111111111111111111111111111111111111111111"
                .to_string(),
            beacon_timestamp: 12345,
            max_epoch: 999,
            claims: vec![TestClaim {
                consolidation_index: 7,
                source_index: 42,
                activation_epoch: 77,
                source_credentials:
                    "0x0200000000000000000000001234567890abcdef1234567890abcdef12345678".to_string(),
                proof_consolidation: vec![
                    "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        .to_string(),
                ],
                proof_credentials: vec![
                    "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                        .to_string(),
                ],
                proof_activation_epoch: vec![
                    "0xcccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                        .to_string(),
                ],
                expected_recipient: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            }],
        }
    }

    #[test]
    fn build_plan_encodes_function_selectors() {
        let plan = build_plan(
            sample_vectors(),
            0,
            "0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02"
                .parse()
                .unwrap(),
            "0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
        )
        .unwrap();

        let set_root_selector = format!(
            "0x{}",
            hex::encode(MockBeaconRootsOracle::setRootCall::SELECTOR)
        );
        let claim_selector = format!(
            "0x{}",
            hex::encode(ConsolidationIncentives::claimRewardCall::SELECTOR)
        );

        assert!(plan.set_root_calldata.starts_with(&set_root_selector));
        assert!(plan.claim_calldata.starts_with(&claim_selector));
        assert!(plan.cast_set_root.contains("setRoot(uint256,bytes32)"));
        assert!(plan.cast_claim.contains(
            "claimReward(uint64,uint64,uint64,uint64,bytes32,bytes32[],bytes32[],bytes32[])"
        ));
        assert!(plan
            .cast_claim
            .contains("[0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa]"));
    }

    #[test]
    fn render_text_includes_recipient_and_addresses() {
        let plan = build_plan(
            sample_vectors(),
            0,
            "0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02"
                .parse()
                .unwrap(),
            "0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
        )
        .unwrap();

        let text = render_text(&plan);
        assert!(text.contains("expected recipient:   0x1234567890abcdef1234567890abcdef12345678"));
        assert!(text.contains("oracle address:       0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02"));
        assert!(text.contains("contract address:     0x1111111111111111111111111111111111111111"));
    }

    #[test]
    fn build_plan_rejects_out_of_range_claim_index() {
        let error = build_plan(
            sample_vectors(),
            99,
            "0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02"
                .parse()
                .unwrap(),
            "0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("claim index 99 out of range"));
    }
}
