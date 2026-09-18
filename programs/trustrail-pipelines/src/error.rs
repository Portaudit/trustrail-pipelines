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
}
