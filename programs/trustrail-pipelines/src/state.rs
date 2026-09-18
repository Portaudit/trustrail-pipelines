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
}
