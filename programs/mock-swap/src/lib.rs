// Copyright 2026 Ishvir and Company (Pty) Ltd
// SPDX-License-Identifier: Apache-2.0

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Mint, Transfer};

declare_id!("3qqN3ZXc1SPEtDLdn9Ek68r9YtDc1mBVgdhS7r48d9vJ");

// Fixed exchange rate. Explicitly arbitrary — no price discovery.
// amount_out = amount_in * RATE_NUMERATOR / RATE_DENOMINATOR
// Documented as mock in the build-log disclosure, not derived from any real market.
pub const RATE_NUMERATOR: u64 = 1;
pub const RATE_DENOMINATOR: u64 = 1; // 1:1 for Tier 0 — change deliberately, not casually

#[program]
pub mod mock_swap {
    use super::*;

    /// One-time setup: seeds mock_pool_output_vault with output-mint liquidity.
    /// Payer mints directly, once, as trusted setup — payer must be output_mint's
    /// actual mint authority. No PDA signature needed since this isn't a
    /// program-signed action.
    pub fn initialize_mock_pool(
        ctx: Context<InitializeMockPool>,
        seed_amount: u64,
    ) -> Result<()> {
        let cpi_accounts = token::MintTo {
            mint: ctx.accounts.output_mint.to_account_info(),
            to: ctx.accounts.mock_pool_output_vault.to_account_info(),
            authority: ctx.accounts.payer.to_account_info(), // real signer, not a PDA
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.key(), cpi_accounts);
        token::mint_to(cpi_ctx, seed_amount)?;

        msg!("Mock pool seeded with {} output tokens", seed_amount);
        Ok(())
    }

    /// The actual swap: staging_ata -> pool_input_vault (debit input),
    /// pool_output_vault -> output_ata (credit output). Both real transfers,
    /// no minting. Fails closed if pool_output_vault can't cover amount_out.
    pub fn mock_swap_execute(ctx: Context<MockSwapExecute>, amount_in: u64) -> Result<()> {
        require!(amount_in > 0, MockSwapError::ZeroAmount);

        require!(
            ctx.accounts.swap_staging_ata.amount >= amount_in,
            MockSwapError::InsufficientStagingBalance
        );

        let amount_out = amount_in
            .checked_mul(RATE_NUMERATOR)
            .and_then(|v| v.checked_div(RATE_DENOMINATOR))
            .ok_or(MockSwapError::RateMathOverflow)?;

        require!(
            ctx.accounts.mock_pool_output_vault.amount >= amount_out,
            MockSwapError::PoolInsufficientLiquidity // fail closed — no minting to cover
        );

        // 1. Debit: swap_staging_ata -> mock_pool_input_vault
        let cpi_accounts_in = Transfer {
            from: ctx.accounts.swap_staging_ata.to_account_info(),
            to: ctx.accounts.mock_pool_input_vault.to_account_info(),
            authority: ctx.accounts.staging_authority.to_account_info(),
        };
        let cpi_ctx_in = CpiContext::new(ctx.accounts.token_program.key(), cpi_accounts_in);
        token::transfer(cpi_ctx_in, amount_in)?;

        // 2. Credit: mock_pool_output_vault -> output_ata
        // Pool vault authority is this program's PDA, so it signs itself.
        let bump = ctx.bumps.pool_authority;
        let seeds: &[&[u8]] = &[b"pool_authority", &[bump]];
        let signer_seeds: &[&[&[u8]]] = &[seeds];

        let cpi_accounts_out = Transfer {
            from: ctx.accounts.mock_pool_output_vault.to_account_info(),
            to: ctx.accounts.output_ata.to_account_info(),
            authority: ctx.accounts.pool_authority.to_account_info(),
        };
        let cpi_ctx_out = CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            cpi_accounts_out,
            signer_seeds,
        );
        token::transfer(cpi_ctx_out, amount_out)?;

        msg!(
            "mock_swap: {} in -> {} out (rate {}/{})",
            amount_in,
            amount_out,
            RATE_NUMERATOR,
            RATE_DENOMINATOR
        );
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeMockPool<'info> {
    #[account(mut)]
    pub payer: Signer<'info>, // must be output_mint's actual mint authority

    #[account(mut)]
    pub output_mint: Account<'info, Mint>,

    #[account(mut)]
    pub mock_pool_output_vault: Account<'info, TokenAccount>,

    /// CHECK: PDA — authority over mock_pool_output_vault as a TOKEN ACCOUNT only.
    /// Not involved in minting; only used later in mock_swap_execute when the
    /// vault needs to sign transfers OUT. Included here so the vault can be
    /// created with this PDA as its owner ahead of time.
    #[account(seeds = [b"pool_authority"], bump)]
    pub pool_authority: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct MockSwapExecute<'info> {
    #[account(mut)]
    pub swap_staging_ata: Account<'info, TokenAccount>,

    /// Authority over swap_staging_ata — the dedicated worker keypair set up
    /// by createStagingAccount. Must sign this call.
    pub staging_authority: Signer<'info>,

    #[account(mut)]
    pub mock_pool_input_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub mock_pool_output_vault: Account<'info, TokenAccount>,

    /// CHECK: PDA that owns mock_pool_output_vault, derived and signed internally.
    #[account(seeds = [b"pool_authority"], bump)]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub output_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[error_code]
pub enum MockSwapError {
    #[msg("amount_in must be greater than zero")]
    ZeroAmount,
    #[msg("swap_staging_ata does not have enough balance for amount_in")]
    InsufficientStagingBalance,
    #[msg("rate math overflowed")]
    RateMathOverflow,
    #[msg("mock pool output vault has insufficient liquidity — no minting to cover")]
    PoolInsufficientLiquidity,
}
