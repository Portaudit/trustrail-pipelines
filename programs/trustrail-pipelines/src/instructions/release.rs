// Copyright 2026 Ishvir and Company (Pty) Ltd
// SPDX-License-Identifier: Apache-2.0

use anchor_lang::prelude::*;
use anchor_spl::token::{self, CloseAccount, Token, TokenAccount, Transfer};

use crate::error::TrustRailError;
use crate::state::{Commitment, CommitmentStatus};

#[derive(Accounts)]
pub struct Release<'info> {
    pub settler: Signer<'info>,
    #[account(
        mut,
        seeds = [b"commitment", commitment.task_id.as_ref()],
        bump = commitment.bump,
    )]
    pub commitment: Account<'info, Commitment>,
    /// CHECK: only a lamport-receiving address, validated against commitment.payer
    #[account(mut, address = commitment.payer)]
    pub payer: UncheckedAccount<'info>,
    #[account(
        mut,
        associated_token::mint = commitment.input_token,
        associated_token::authority = commitment,
    )]
    pub escrow_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        associated_token::mint = commitment.output_token,
        associated_token::authority = commitment,
    )]
    pub output_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        associated_token::mint = commitment.output_token,
        associated_token::authority = commitment.payer,
    )]
    pub payer_output_ata: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<Release>) -> Result<()> {
    let (task_id, bump) = {
        let c = &mut ctx.accounts.commitment;
        require!(c.status == CommitmentStatus::Passed, TrustRailError::WrongState);
        c.status = CommitmentStatus::Released;
        (c.task_id, c.bump)
    };
    let commitment_seeds: &[&[u8]] = &[b"commitment", task_id.as_ref(), &[bump]];
    let signer_seeds: &[&[&[u8]]] = &[commitment_seeds];

    let output_amount = ctx.accounts.output_ata.amount;
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.output_ata.to_account_info(),
                to: ctx.accounts.payer_output_ata.to_account_info(),
                authority: ctx.accounts.commitment.to_account_info(),
            },
            signer_seeds,
        ),
        output_amount,
    )?;

    token::close_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.key(),
        CloseAccount {
            account: ctx.accounts.output_ata.to_account_info(),
            destination: ctx.accounts.payer.to_account_info(),
            authority: ctx.accounts.commitment.to_account_info(),
        },
        signer_seeds,
    ))?;

    // fails loudly if a future swap step left escrow non-empty — see prose above
    token::close_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.key(),
        CloseAccount {
            account: ctx.accounts.escrow_ata.to_account_info(),
            destination: ctx.accounts.payer.to_account_info(),
            authority: ctx.accounts.commitment.to_account_info(),
        },
        signer_seeds,
    ))?;

    Ok(())
}
