use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::error::TrustRailError;
use crate::state::{Commitment, CommitmentStatus};

#[derive(Accounts)]
pub struct WithdrawForSwap<'info> {
    pub settler: Signer<'info>,
    #[account(
        mut,
        seeds = [b"commitment", commitment.task_id.as_ref()],
        bump = commitment.bump,
    )]
    pub commitment: Account<'info, Commitment>,
    #[account(
        mut,
        associated_token::mint = commitment.input_token,
        associated_token::authority = commitment,
    )]
    pub escrow_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = swap_staging_ata.key() == commitment.expected_staging_ata @ TrustRailError::StagingAtaMismatch,
        constraint = swap_staging_ata.mint == escrow_ata.mint @ TrustRailError::MintMismatch,
    )]
    pub swap_staging_ata: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<WithdrawForSwap>) -> Result<()> {
    let (task_id, bump) = {
        let c = &mut ctx.accounts.commitment;
        require!(c.status == CommitmentStatus::Locked, TrustRailError::WrongState);
        require!(!c.escrow_withdrawn, TrustRailError::EscrowAlreadyWithdrawn);
        (c.task_id, c.bump)
    };
    let commitment_seeds: &[&[u8]] = &[b"commitment", task_id.as_ref(), &[bump]];
    let signer_seeds: &[&[&[u8]]] = &[commitment_seeds];

    let escrow_amount = ctx.accounts.escrow_ata.amount;
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.escrow_ata.to_account_info(),
                to: ctx.accounts.swap_staging_ata.to_account_info(),
                authority: ctx.accounts.commitment.to_account_info(),
            },
            signer_seeds,
        ),
        escrow_amount,
    )?;

    ctx.accounts.commitment.escrow_withdrawn = true;

    Ok(())
}
