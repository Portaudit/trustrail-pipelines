use anchor_lang::prelude::*;

#[error_code]
pub enum TrustRailError {
    WrongState,
    UnauthorizedWorker,
    ClaimMismatch,
    SlippageFloorNotMet,
    ProofAlreadyStamped,
    DeadlineNotReached,
    DeadlinePassed,
    TokenMintMismatch,
    Overflow,
    #[msg("Escrow has already been withdrawn for swap")]
    EscrowAlreadyWithdrawn,
    #[msg("Swap staging ATA does not match the address recorded at commitment creation")]
    StagingAtaMismatch,
    #[msg("Swap staging ATA mint does not match escrow mint")]
    MintMismatch,
}
