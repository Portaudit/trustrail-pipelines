// Copyright 2026 Ishvir and Company (Pty) Ltd
// SPDX-License-Identifier: Apache-2.0

use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use crate::error::TrustRailError;
use crate::state::{Commitment, CommitmentStatus};

#[derive(Accounts)]
#[instruction(task_id: [u8; 32])]
pub struct CreateCommitment<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: executor is recorded at creation, not required to sign
    pub executor_agent: UncheckedAccount<'info>,
    pub input_token: Account<'info, Mint>,
    pub output_token: Account<'info, Mint>,
    #[account(
        init, payer = payer,
        space = 8 + Commitment::INIT_SPACE,
        seeds = [b"commitment", task_id.as_ref()], bump,
    )]
    pub commitment: Account<'info, Commitment>,
    #[account(init, payer = payer,
        associated_token::mint = input_token, associated_token::authority = commitment)]
    pub escrow_ata: Account<'info, TokenAccount>,
    #[account(init, payer = payer,
        associated_token::mint = output_token, associated_token::authority = commitment)]
    pub output_ata: Account<'info, TokenAccount>,
    #[account(mut,
        associated_token::mint = input_token, associated_token::authority = payer)]
    pub payer_input_ata: Account<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

pub fn handler(
    ctx: Context<CreateCommitment>,
    task_id: [u8; 32],
    executor_agent: Pubkey,
    input_amount: u64,
    min_output_amount: u64,
    deadline_slot: u64,
    expected_staging_ata: Pubkey,
) -> Result<()> {
    require!(input_amount > 0 && min_output_amount > 0, TrustRailError::WrongState);

    let c = &mut ctx.accounts.commitment;
    c.task_id = task_id;
    c.payer = ctx.accounts.payer.key();
    c.executor_agent = executor_agent;
    c.input_token = ctx.accounts.input_token.key();
    c.output_token = ctx.accounts.output_token.key();
    c.input_amount = input_amount;
    c.min_output_amount = min_output_amount;
    c.deadline_slot = deadline_slot;
    c.external_verdict = None;
    c.proof_tx = None;
    c.proof_slot = None;
    c.verified = false;
    c.status = CommitmentStatus::Locked;
    c.bump = ctx.bumps.commitment;
    c.escrow_withdrawn = false;
    c.expected_staging_ata = expected_staging_ata;

    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.payer_input_ata.to_account_info(),
                to: ctx.accounts.escrow_ata.to_account_info(),
                authority: ctx.accounts.payer.to_account_info(),
            },
        ),
        input_amount,
    )
}
