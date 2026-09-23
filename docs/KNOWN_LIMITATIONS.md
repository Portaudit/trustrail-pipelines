# Known Limitations & Disclosures

This document tracks known gaps, deliberate simplifications, and honest disclosures
about the current state of the TrustRail on-chain program and its supporting worker
scripts. It is maintained as work progresses rather than written retroactively, so
some items may be resolved by the time you read this — check the date each entry
was added against the commit history if that matters to you.

## Mock swap venue (`mock_swap` program)

`mock_swap` is a deliberate stand-in for a real swap venue (e.g. Jupiter), built to
satisfy an internal September 28, 2026 gate requirement for a real, on-chain,
non-trivial swap step in the settlement pipeline. It is **not** a production swap
integration.

- The swap rate is fixed at 1:1 (`RATE_NUMERATOR = 1`, `RATE_DENOMINATOR = 1`), not
  derived from any real market or oracle price.
- Liquidity is seeded once via `initialize_mock_pool`, minted directly by a trusted
  signer (`payer`) rather than sourced from any real pool.
- `mock_swap_execute` does perform a real, on-chain token transfer of the actual
  escrowed input amount (staging ATA -> pool input vault) and a real transfer of
  output tokens (pool output vault -> output ATA) — it is not merely minting output
  tokens into existence. This was a deliberate design requirement, not an incidental
  property.
- TrustRail's critical settlement path (`create_commitment`, `withdraw_for_swap`,
  `submit_proof`) contains no swap-venue-specific logic and no LLM/semantic judgment
  of any kind — `mock_swap` is swappable for a real venue integration without
  changing any of TrustRail's own on-chain verification logic.

## Stuck-funds gap after `withdraw_for_swap`

Once `withdraw_for_swap` succeeds, `escrow_withdrawn` is set to `true` and the
`Commitment`'s escrow account is empty; the input tokens now live in
`swap_staging_ata`, owned by a per-commitment staging keypair. If the subsequent
swap step never executes or never completes (e.g. the swap venue call fails, or the
worker process crashes between withdrawal and swap), there is currently **no
recovery instruction** to move those staged funds back into the escrow or return
them to the original payer. `refund` and `cancel` both correctly guard against this
state (`require!(!escrow_withdrawn, ...)`), so they fail loudly rather than silently
sweeping an already-drained escrow account — but there is no path forward for that
stuck state yet. This is a known, real gap, not yet closed.

## `ProofAlreadyStamped` error variant

`TrustRailError::ProofAlreadyStamped` is defined but not currently reachable from
any instruction path — `submit_proof`'s existing status-based guards
(`WrongState`) prevent double-submission before this variant's check would ever be
reached. It is effectively dead code today. Left in place rather than removed,
since it documents original intent and may become reachable if `submit_proof`'s
guard ordering changes.

## Local `litesvm` integration test suite (`tests/flow.rs`) is currently broken

The in-process integration tests in `programs/trustrail-pipelines/tests/flow.rs`
(covering `create_commitment` and `submit_proof` happy/unhappy paths) currently fail
at setup with `add_program failed: Instruction(InvalidAccountData)`. Root cause,
confirmed against a real upstream issue: the pinned `litesvm = "0.10.0"` cannot
parse the sBPFv3 ELF format emitted by the current build toolchain (same failure
class as [brimigs/anchor-litesvm#3](https://github.com/brimigs/anchor-litesvm/issues/3)).
Confirmed **not** a devnet-affecting issue — it is purely a local test-harness
incompatibility. A trial upgrade to `litesvm = "0.16"` fixes the ELF-parsing issue
but surfaces a separate multi-crate `solana-*` dependency version conflict
(`solana-transaction` 3.1.0 vs 4.1.6) that requires bumping several `solana-*`
dev-dependencies in lockstep. Deferred: real on-chain devnet verification (see repo
README / commit history for explorer links) already independently covers both the
`Passed` and `FailedSlippage` paths this test suite is meant to check, so this was
not treated as blocking before the September 28, 2026 gate.

## Fixed-mint model / commitment independence

`create-commitment.ts` uses a fixed, persisted `input_mint`/`output_mint` pair
(`worker/mints.json`), created once rather than freshly minted per run. This is
required because `initialize-mock-pool.ts` is one-time setup for a fixed mint pair;
re-minting per run would desync the mock pool from new commitments. Independence
between separate commitment runs comes from `task_id` varying (each `Commitment`
PDA derives from `seeds = [b"commitment", task_id]`), not from mints varying.

## Four independent copies of `MOCK_SWAP_PROGRAM_ID`

The `mock_swap` program ID appears as four separate hardcoded literals: in
`programs/mock-swap/src/lib.rs`'s `declare_id!`, `Anchor.toml`'s `[programs.*]`
section, `worker/src/mock-swap.ts`, and `worker/src/initialize-mock-pool.ts`.
Nothing cross-checks these against each other; a typo in any one produces a
generic "program not found" or wrong-owner error rather than a named one.
