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
    ) -> Result<()> {
        instructions::create_commitment::handler(
            ctx, task_id, executor_agent, input_amount, min_output_amount, deadline_slot)
    }
}
