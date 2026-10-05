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
`swap_staging_ata`, owned by a per-commitment staging keypair. By their source,
`refund` and `cancel` both require `!escrow_withdrawn`, so neither can act on
this state.

Status as of the program upgrade at slot 507558273 (`trustrail_pipelines`).

### Closed, verified on devnet: FailedSlippage with the escrow withdrawn

The `recover` instruction handles this state. It returns the proceeds left in the
commitment's output account to the payer and closes the commitment's escrow and
output accounts. It returns the swap output token only, not the original input,
so it is recovery of stuck proceeds, not a refund.

It was run once on devnet against Run B (commitment
`HAi7f8Qf5AsJCSre5ruqMZQ6RchLG6TTuUPKogHMyiPA`), signed by a settler that is not
the payer. Before: escrow 0, output 300000, staging 0. After: status Recovered,
escrow and output accounts closed, staging 0, payer output account 300000, payer
input account unchanged. Recover tx
`2LZ3VqM99NPCW5bDeSjXxfQVm2P5WvLUpY6Y6CEv4G5UjftGHtPsbTMZWsfb96cK5t2V91LZhbuCWzufahpr4R2C`.
Run B's five original transactions are unchanged.

### Still open

- **Locked with the escrow withdrawn.** `recover` rejects this state with
  `WrongState`. Handling it is planned for Slice 2.
- **Non-empty staging account.** The worker currently creates staging accounts
  with no delegate, so `recover` cannot pull funds out of a non-empty staging
  account yet. The litesvm test `recover_rejects_unpullable_staging_atomically`
  covers the failure case. The Run B recovery had an empty staging account.

### Limits of the evidence

- The litesvm `recover` tests set commitment and token-account states directly.
  They show how `recover` behaves from those states, not that the real
  instruction chain reaches them. Only the devnet run on Run B shows that the real
  chain reaches the FailedSlippage-with-withdrawn state.
- `release`, `refund` and `cancel` exist in the program but have not been
  exercised in the litesvm tests or in either documented devnet run (Runs A and B). Runs A and B ended Passed
  and Recovered.

### Deployed binary

sha256 `7dcd90d415a6e5bf1304b127c64d0a834a4b54934b745d76650b65a20316dee5`. It was
built locally with `anchor build` at 01:50:11 (+0200) on 2026-10-05 from the
sources of commit `80c3d18`; no tracked source file had a later modification
time. This is not a reproducible-build claim.

## `ProofAlreadyStamped` error variant

`TrustRailError::ProofAlreadyStamped` is defined but not currently reachable from
any instruction path — `submit_proof`'s existing status-based guards
(`WrongState`) prevent double-submission before this variant's check would ever be
reached. It is effectively dead code today. Left in place rather than removed,
since it documents original intent and may become reachable if `submit_proof`'s
guard ordering changes.

## Local `litesvm` integration test suite (`tests/flow.rs`) -- resolved

Was broken: the in-process integration tests failed at setup with
`add_program failed: Instruction(InvalidAccountData)`, because the pinned
`litesvm = "0.10.0"` could not parse the sBPFv3 ELF format emitted by the
current build toolchain (same failure class as
[brimigs/anchor-litesvm#3](https://github.com/brimigs/anchor-litesvm/issues/3)).

Fix: `litesvm` bumped to `0.16.0`. An initial attempt at this bump in place
broke `anchor build`'s IDL-generation step instead -- `anchor build` resolves
the entire anchor workspace's unified `Cargo.lock`, so the dependency bump
reached it too, hitting the same underlying rustc/`maybe_uninit_write_slice`
issue on a different surface. Resolved properly by moving the litesvm tests
into `tests-litesvm/`, a separate Cargo workspace (own `Cargo.lock`, excluded
from the root `[workspace]`) whose dev-dependencies can never again reach
`anchor build`'s resolution. Verified: `cd tests-litesvm && cargo test`
(6/6 pass) and `anchor build` from root (exit 0, no `tests-litesvm`
involvement) both pass independently.

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
