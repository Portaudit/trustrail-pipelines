// Copyright 2026 Ishvir and Company (Pty) Ltd
// SPDX-License-Identifier: Apache-2.0

use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, Default, InitSpace)]
pub enum CommitmentStatus {
    #[default]
    Locked,
    Passed,
    FailedSlippage,
    Released,
    Refunded,
    TimedOut,
}

#[account]
#[derive(InitSpace)]
pub struct Commitment {
    pub task_id: [u8; 32],
    pub payer: Pubkey,
    pub executor_agent: Pubkey,
    pub input_token: Pubkey,
    pub output_token: Pubkey,
    pub input_amount: u64,
    pub min_output_amount: u64,
    pub deadline_slot: u64,
    pub external_verdict: Option<Pubkey>,
    pub proof_tx: Option<[u8; 64]>,
    pub proof_slot: Option<u64>,
    pub verified: bool,
    pub status: CommitmentStatus,
    pub bump: u8,
    pub escrow_withdrawn: bool,
    pub expected_staging_ata: Pubkey,
}

#[cfg(test)]
mod space_check {
    use super::*;

    // Field-by-field manual tally, computed by hand from anchor_lang's Space impls
    // (Pubkey=32, u64=8, bool=1, [u8;N]=N, Option<T>=1+size_of::<T>(), fieldless
    // enum=1). Before this diff (with escrow_withdrawn already present, but
    // without expected_staging_ata) the same tally comes to 295. Adding one
    // Pubkey field should grow INIT_SPACE by exactly 32, to 327 — this asserts
    // the derive actually did that rather than trusting it silently.
    const EXPECTED_BEFORE_THIS_FIELD: usize = 295;
    const EXPECTED_AFTER_THIS_FIELD: usize = EXPECTED_BEFORE_THIS_FIELD + 32; // Pubkey

    #[test]
    fn init_space_grew_by_exactly_one_pubkey() {
        assert_eq!(
            Commitment::INIT_SPACE,
            EXPECTED_AFTER_THIS_FIELD,
            "Commitment::INIT_SPACE = {}, expected {} (295 pre-expected_staging_ata + 32 for the new Pubkey field). \
             If this fails, the derive did NOT grow space the way assumed — stop and investigate before anchor build.",
            Commitment::INIT_SPACE,
            EXPECTED_AFTER_THIS_FIELD,
        );
    }
}
