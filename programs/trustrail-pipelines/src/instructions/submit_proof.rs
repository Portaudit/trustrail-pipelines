// Copyright 2026 Ishvir and Company (Pty) Ltd
// SPDX-License-Identifier: Apache-2.0

use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;

use crate::error::TrustRailError;
use crate::state::{Commitment, CommitmentStatus};

#[derive(Accounts)]
pub struct SubmitProof<'info> {
    pub worker: Signer<'info>,
    #[account(
        mut,
        seeds = [b"commitment", commitment.task_id.as_ref()],
        bump = commitment.bump,
    )]
    pub commitment: Account<'info, Commitment>,
    #[account(
        associated_token::mint = commitment.output_token,
        associated_token::authority = commitment,
    )]
    pub output_ata: Account<'info, TokenAccount>,
}

pub fn handler(
    ctx: Context<SubmitProof>,
    proof_tx: [u8; 64],
    proof_slot: u64,
    claimed_actual_output: u64,
) -> Result<()> {
    let c = &mut ctx.accounts.commitment;

    require!(
        ctx.accounts.worker.key() == c.executor_agent,
        TrustRailError::UnauthorizedWorker
    );
    require!(c.status == CommitmentStatus::Locked, TrustRailError::WrongState);
    require!(c.proof_tx.is_none(), TrustRailError::ProofAlreadyStamped);

    let actual = ctx.accounts.output_ata.amount;
    require!(claimed_actual_output == actual, TrustRailError::ClaimMismatch);

    c.proof_tx = Some(proof_tx);
    c.proof_slot = Some(proof_slot);
    c.verified = true;

    c.status = if actual >= c.min_output_amount {
        CommitmentStatus::Passed
    } else {
        CommitmentStatus::FailedSlippage
    };

    Ok(())
}
