use anchor_lang::prelude::*;

declare_id!("CgXidrtsV5nLkjPCYhZUPUuZUpmvekMKvDN9uh3ucoC8");

pub mod error;
pub mod instructions;
pub mod state;
use instructions::*;

#[program]
pub mod trustrail_pipelines {
    use super::*;
    pub fn create_commitment(
        ctx: Context<CreateCommitment>,
        task_id: [u8; 32],
        executor_agent: Pubkey,
        input_amount: u64,
        min_output_amount: u64,
        deadline_slot: u64,
        expected_staging_ata: Pubkey,
    ) -> Result<()> {
        instructions::create_commitment::handler(
            ctx, task_id, executor_agent, input_amount, min_output_amount, deadline_slot, expected_staging_ata)
    }

    pub fn submit_proof(
        ctx: Context<SubmitProof>,
        proof_tx: [u8; 64],
        proof_slot: u64,
        claimed_actual_output: u64,
    ) -> Result<()> {
        instructions::submit_proof::handler(ctx, proof_tx, proof_slot, claimed_actual_output)
    }

    pub fn release(ctx: Context<Release>) -> Result<()> {
        instructions::release::handler(ctx)
    }

    pub fn refund(ctx: Context<Refund>) -> Result<()> {
        instructions::refund::handler(ctx)
    }

    pub fn cancel(ctx: Context<Cancel>) -> Result<()> {
        instructions::cancel::handler(ctx)
    }

    pub fn withdraw_for_swap(ctx: Context<WithdrawForSwap>) -> Result<()> {
        instructions::withdraw_for_swap::handler(ctx)
    }
}
