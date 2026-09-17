# TrustRail — Tier 0 schema memo (locked 2026-09-17, rev 2 2026-09-18)

## Accounts
- Commitment PDA: seeds=["commitment", task_id]
- Output ATA: PDA-owned, seeds=["out", commitment, mint]; created at create_commitment;
  starts empty; its balance IS the on-chain output fact.

## Commitment
- task_id [u8;32] · payer · executor_agent · input_token · output_token
- input_amount · min_output_amount (slippage floor) · deadline_slot
- external_verdict: Option<Pubkey>   # v1 always None; reserved for mock-verifier adapters
- proof_tx: Option<[u8;64]> · proof_slot: Option<u64>   # audit pointer only; program never verifies sig contents
- verified: bool · status · bump

## Statuses
Locked | Passed | FailedSlippage | Released | Refunded | TimedOut

## Instructions
1. create_commitment(payer) -> Locked; state first, then deposit CPI; derives output ATA.
2. submit_proof(executor) -> requires Locked; stores proof_tx/proof_slot (pointer for auditors).
3. verify(worker) -> requires Locked && !verified; NO CPI.
   - Reads output ATA balance as account state.
   - Arg: claimed_actual_output; cross-check claim == balance else ClaimMismatch.
   - Verdict: balance >= min_output_amount -> Passed else FailedSlippage.
   - Writes VerificationResult (claim, balance, sig ref, verifier, slot).
4. release -> requires Passed; state first, then CPIs: input escrow -> executor, output ATA -> payer.
5. refund -> requires FailedSlippage; state first, then CPIs: input escrow -> payer, output ATA -> executor.
6. cancel(anyone) -> requires Locked && !verified && slot >= deadline; TimedOut then CPI input -> payer.

## Trust boundary (rev 2)
- Worker controls liveness, not truth: false claim -> ClaimMismatch; silent worker -> cancel.
- Verdict input is account state the program reads itself; PDA derivation makes the output account unspoofable.
- Direct deposits are not a bypass: funding the output ATA costs the depositor the delivered amount.
  Settlement prices delivery; proof_tx evidences method.
- No judge model, no oracle, no CPI inside verify.

## Errors (#[error_code])
WrongState · UnauthorizedWorker · ClaimMismatch · SlippageFloorNotMet ·
ProofAlreadyStamped · DeadlineNotReached · DeadlinePassed · TokenMintMismatch · Overflow

## Invariants
- State mutation ALWAYS precedes CPI (kills double-action).
- Terminal statuses reject re-entry (idempotency).
- v1 verdict = program arithmetic over on-chain balances; re-derivable from explorer by anyone.
