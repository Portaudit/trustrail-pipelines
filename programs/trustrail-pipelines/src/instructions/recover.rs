// Copyright 2026 Ishvir and Company (Pty) Ltd
// SPDX-License-Identifier: Apache-2.0

use anchor_lang::prelude::*;
use anchor_spl::token::{self, CloseAccount, Token, TokenAccount, Transfer};

use crate::error::TrustRailError;
use crate::state::{Commitment, CommitmentStatus};

/// True when the commitment PDA can move `amount` out of `staging`:
/// nothing to pull, PDA owns it, or PDA is delegate with enough allowance.
pub fn staging_pullable(
    staging_owner: &Pubkey,
    staging_delegate: Option<Pubkey>,
    delegated_amount: u64,
    commitment_key: &Pubkey,
    amount: u64,
) -> bool {
    amount == 0
        || staging_owner == commitment_key
        || (staging_delegate == Some(*commitment_key) && delegated_amount >= amount)
}

#[derive(Accounts)]
pub struct Recover<'info> {
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
        constraint = swap_staging_ata.key() == commitment.expected_staging_ata
            @ TrustRailError::StagingAtaMismatch,
        constraint = swap_staging_ata.mint == commitment.input_token
            @ TrustRailError::StagingAtaMintMismatch,
    )]
    pub swap_staging_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        associated_token::mint = commitment.input_token,
        associated_token::authority = commitment.payer,
    )]
    pub payer_input_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        associated_token::mint = commitment.output_token,
        associated_token::authority = commitment.payer,
    )]
    pub payer_output_ata: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<Recover>) -> Result<()> {
    let (task_id, bump) = {
        let c = &mut ctx.accounts.commitment;
        // Branch 2: FailedSlippage with escrow already withdrawn, no deadline.
        // Branch 1: Locked with escrow withdrawn and no proof stamped, only
        // strictly after the deadline. Everything else is WrongState.
        let branch2 = c.status == CommitmentStatus::FailedSlippage && c.escrow_withdrawn;
        if !branch2 {
            require!(
                c.status == CommitmentStatus::Locked
                    && c.escrow_withdrawn
                    && c.proof_tx.is_none(),
                TrustRailError::WrongState
            );
            require!(
                Clock::get()?.slot > c.deadline_slot,
                TrustRailError::DeadlineNotReached
            );
        }
        c.status = CommitmentStatus::Recovered;
        (c.task_id, c.bump)
    };
    let commitment_seeds: &[&[u8]] = &[b"commitment", task_id.as_ref(), &[bump]];
    let signer_seeds: &[&[&[u8]]] = &[commitment_seeds];
    let commitment_key = ctx.accounts.commitment.key();

    let staging_amount = ctx.accounts.swap_staging_ata.amount;
    let escrow_amount = ctx.accounts.escrow_ata.amount;
    let output_amount = ctx.accounts.output_ata.amount;

    if staging_amount > 0 {
        let st = &ctx.accounts.swap_staging_ata;
        require!(
            staging_pullable(
                &st.owner,
                Option::<Pubkey>::from(st.delegate),
                st.delegated_amount,
                &commitment_key,
                staging_amount,
            ),
            TrustRailError::StagingNotPullable
        );
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.swap_staging_ata.to_account_info(),
                    to: ctx.accounts.payer_input_ata.to_account_info(),
                    authority: ctx.accounts.commitment.to_account_info(),
                },
                signer_seeds,
            ),
            staging_amount,
        )?;
    }

    if escrow_amount > 0 {
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.escrow_ata.to_account_info(),
                    to: ctx.accounts.payer_input_ata.to_account_info(),
                    authority: ctx.accounts.commitment.to_account_info(),
                },
                signer_seeds,
            ),
            escrow_amount,
        )?;
    }
    token::close_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.key(),
        CloseAccount {
            account: ctx.accounts.escrow_ata.to_account_info(),
            destination: ctx.accounts.payer.to_account_info(),
            authority: ctx.accounts.commitment.to_account_info(),
        },
        signer_seeds,
    ))?;

    if output_amount > 0 {
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
    }
    token::close_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.key(),
        CloseAccount {
            account: ctx.accounts.output_ata.to_account_info(),
            destination: ctx.accounts.payer.to_account_info(),
            authority: ctx.accounts.commitment.to_account_info(),
        },
        signer_seeds,
    ))?;

    Ok(())
}
