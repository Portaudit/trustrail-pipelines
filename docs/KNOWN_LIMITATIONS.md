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

Once `withdraw_for_swap` succeeds, `escrow_withdrawn` is `true` and the
`Commitment`'s escrow account is empty; the input tokens live in the swap staging
account. By their source, `refund` and `cancel` both require `!escrow_withdrawn`,
so neither can act on that state. The `recover` instruction handles it.

Deployed program (devnet): `CgXidrtsV5nLkjPCYhZUPUuZUpmvekMKvDN9uh3ucoC8`. The
current deployment is the Slice 3 binary (see "Slice 3" below). The previous
(Slice 2) deployment was source commit `457165a`, last deployed slot 507592632, upgrade tx `3NjV86VemtvZCHYRegmJwvMACcQ7CNHqbYep6HkuoBACKtJhzFE7wSzsfQinpWFJHD84X4nZbUsKo35HnrkrmC4x`. Binary:
264880 bytes, sha256
`c4c5421712552c1c864e85c07539c6fc7297989cdee056bd4ad551aadbbb7e0d`. Provenance is
by file timestamps and a dump-hash check after the upgrade; this is not a
reproducible-build claim.

### What the program does now

- **`recover`, FailedSlippage with the escrow withdrawn.** No deadline. It
  returns the swap output left in the commitment's output account (and any input
  left in escrow or staging) to the payer and closes the commitment's escrow and
  output accounts. When only swap output comes back, this is recovery of stuck
  proceeds, not a refund.
- **`recover`, Locked with the escrow withdrawn and no proof stamped.** Only when
  the current slot is strictly after `deadline_slot`. Before that it fails
  `DeadlineNotReached` (6005, 0x1775). Any other state fails `WrongState` (6000);
  the `WrongState` check runs first.
- **`withdraw_for_swap` guard.** Before moving funds it requires the staging
  account to be pullable by the commitment: staging owned by the commitment PDA,
  or the commitment PDA as delegate with an allowance of at least the escrow
  amount. Otherwise it fails `StagingNotPullable` (6011, 0x177B) and nothing moves.
- **Worker.** Each new staging account approves the commitment PDA as delegate
  for `u64::MAX`, and the worker checks the readback. The allowance is "approved
  for `u64::MAX`", not a fixed balance: delegate pulls reduce it, and owner
  transfers do not change it. Evidence: litesvm test B6, and on devnet staging
  `FsHyhXq2P3kmyqeo8urbyxiTm9Gsi3vVpmsFeTwitPNL` shows 18446744073709301615
  after a 250000 pull.

### Verified on devnet

All signatures below were confirmed finalized. Addresses are commitments.

- **Run B** (FailedSlippage, escrow withdrawn): recovered by a settler that is
  not the payer. Before: escrow 0, output 300000, staging 0. After: status
  Recovered, escrow and output accounts closed, staging 0, payer output account
  300000, payer input unchanged. Commitment
  `HAi7f8Qf5AsJCSre5ruqMZQ6RchLG6TTuUPKogHMyiPA`, recover tx
  `2LZ3VqM99NPCW5bDeSjXxfQVm2P5WvLUpY6Y6CEv4G5UjftGHtPsbTMZWsfb96cK5t2V91LZhbuCWzufahpr4R2C`.
- **Run C** (`4ywZV4cW67iic82FcY3bcxzPamvczxeMXx88ep9tAFFu`): input 250000,
  minimum output 500000, 900-slot deadline (create landed in slot 507593103,
  deadline 507593999). The staging key was moved out of the worker folder first.
  `withdraw_for_swap` moved 250000 from escrow to staging. An early `recover` at
  slot 507593356 failed `0x1775` and changed nothing. After the deadline, a
  settler that is not the payer recovered it in slot 507594044: payer input
  700000 to 950000, escrow and output closed, staging 0, status Recovered, with
  no worker key involved. A second `recover` failed `AccountNotInitialized`
  (3012, 0xbc4) on the escrow account, and the status stayed Recovered.
  - create `2dJPE5YxrYGD4b5dQe8S7xJJb5x3VGUhhN7fURaG9TWCQLrbrA7WzRPfWZ6LGooN7R4RJdvLUNMHeT3pB8rrXMPM`
  - withdraw `5FX7Zx2DjYx4fj5ZY9rfn6U64c7UiQjv7iJCeUTJghTKNYKL9mKpyr2BQMPaj2YdQTp7rRwF7GB1dLvQr6t6pQaE`
  - recover `5ZuvUcFtCuzqVnFGJZDJWVF5biQFGVrQHBdUse9UnzS4kG57P8431GrKL4JV1bNW3utXM1KbBKCJtzb7jUNUoUVi`
- **Run C2** (`ByYUeb7Fr2nbLpf6n75dgK5oq2WttUhMBDxqe3it2eXV`): input 200000,
  minimum output 500000, 1500-slot deadline. The staging key revoked the
  delegate; `withdraw_for_swap` then failed `StagingNotPullable` (0x177B) in
  simulation, so nothing was sent and escrow still held 200000. After the
  deadline, `cancel` by a settler that is not the payer (slot 507596533): escrow
  200000 to closed, payer input 950000 to 1150000, status TimedOut.
  - create `5P4tbsW1gegpsK3fqwnRN2orp62zHtpuxddhNKS8uHM1iiUBJF6yMJtJPf9Y3RLdBvViHAC11ENC13PYnGVp8vyY`
  - revoke `33wR5cPPjnjdMBT3uB4fZGBqLwsqKy9DhdgzyUZFwuXWzLE4cz22rCbLDAvBY6rwbnks2kRTBmzZyiuRHr7AeYnf`
  - cancel `CaRXK5SUBowfrbU8zvr3CLtZNfyVBqU8CsfoAjnTtkosyyd1X2YJXgUxM6v8s6bPUCgGyHqbybxyWe4CZux9LfG`
- **Run D** (`5zrxT6AdAfms6UbYKipi4AVxo4Yp4cXXNmN9RoyPhre2`): the normal path with
  the delegate in place reached Passed. It was left unreleased on purpose, then released in Slice 3 (below). The
  `proofTx` field in the decoded account is random bytes, not a transaction.
  - create `LFLceLhx8KCH4CuRft7cgYnivCnZ4GxcQSt9zAbuPFwcmuVBnNtBAed2hKYCd2dxqcexBMCrSAMhoksscuNK1tY`
  - withdraw `22YP1vqJQUtxALj1HoCkoAaaRQsZKdhpX48aaMD5zx8ZPH637CiFkRyHP6ipvY7FjdXHA7ViUBGXKpDfWmiKEPRr`
  - swap `3NmMnH1fYnCzu3zBz2reBvq3RMNMDjaZXU97JVUpW9zCEvCt5NUVVNpUKZ1LvzgbPXQXpY3SL32oqL4NSjucbQbU`
  - submit_proof `66LyJSn2YuSb8KmaTiBfZht91LJeTrtjKwPmg9bAxi3415zFDzBBU4GPyDBjdhcCqzCn7dMu29i1UXwnum2qvLVM`

### Still open

- **Legacy staging accounts.** Recovery from a non-empty staging account works
  only for staging accounts created with the delegate (the current worker).
  Staging accounts created without a delegate recover only if staging is empty.
- **Hostile staging-key holder.** The key holder can revoke the delegate:
  `withdraw_for_swap` then refuses, and if the revoke comes before the withdraw
  the funds stay recoverable through `cancel` after the deadline (Run C2). If the
  delegate is revoked after the withdraw, `recover` cannot pull non-empty staging
  (`StagingNotPullable`, litesvm test B5), and the staging key holder owns that
  account in any case. The key holder can also close the staging
  account; that makes `recover` fail with `AccountNotInitialized` in both
  branches. This is tested in litesvm (`recover_fails_when_staging_closed`) and
  has not been run on devnet.
- **Exact-amount allowance.** If the allowance is an exact amount rather than
  `u64::MAX`, a donation into staging makes `recover` fail with
  `StagingNotPullable`. Tested in litesvm; the worker always approves `u64::MAX`.
- **S3 (staging remainder).** After a run reaches Passed or Released, any
  remainder left in the swap staging account stays there. Open.
- **S4 (donated output forces Passed).** `submit_proof` never reads the escrow,
  so tokens donated into the output account can push it to the minimum and the
  executor's `submit_proof` records Passed while the escrow is still non-empty.
  Before Slice 3, `release` then failed at the escrow close (token error 0xb)
  and no other exit remained. Reproduced in litesvm against the pre-sweep
  program. The `release` sweep now moves any escrow remainder to the payer
  before the close, covered in litesvm; the output-donation variant has not been
  reproduced on devnet. Only the executor can call `submit_proof`, so this
  variant needs the executor or a colluding donor. Closed for `release` by the
  sweep. Passed still attests nothing about a swap (see below).
- **Dust variant (closed by the sweep).** After `withdraw_for_swap`, anyone can
  send one input token to a Passed commitment's escrow account, which made
  `release` fail at the escrow close. Reproduced in simulation against the
  deployed Slice 2 binary on devnet (token error 0xb, nothing sent). After the
  Slice 3 upgrade the same account released and the payer received the donated
  unit. A third party could cause this at the cost of one token unit.
- **`expected_staging_ata` is not validated at creation.** `create_commitment`
  stores the address the caller passes without checking it, so it can be any
  account, including one that is not a distinct staging account. `withdraw_for_swap`
  and `recover` only require the staging account to equal that stored address and
  to have the input mint. The behaviour when the stored address is the
  commitment's own escrow account has not been tested. Open.
- **What Passed means.** Passed attests only that the output account held at
  least the minimum, not that a swap happened.
- **Legacy commitment accounts.** Three 302-byte commitment accounts on devnet
  predate the current layout, cannot be decoded by the current program, and are
  not covered by any recovery claim. It is not claimed that every devnet
  commitment is recoverable.

### Slice 3: release dust sweep (verified on devnet, Oct 7)

- `release` now takes `payer_input_ata` as a trailing eighth account and sweeps
  any input token left in escrow to the payer before closing it. Program source
  built from commit `7ede6d0530f984eb6c3ee7d10331e0e35133e872` (its program
  source is the same as `e17a072`; `7ede6d0` only adds `worker/src/release.ts`).
  Binary 268368 bytes, sha256
  `a50c98d07dc0bdaf7aa560d3ff4f2b5246bb2e447f81069e4f61bda96f40a798`, built
  locally; not a reproducible build.
- Upgrade tx `43bV1pbL4aXK3zVTjskPsRBQqyFv68KLp8cTyLnBtYYD9JCmaAgBcMiUYnXTnu8wAp6tC8cUiC2M5T1ZEA9Q8EJC`
  (Finalized), last deployed slot 508283076. A buffer dump matched the sha256
  before the upgrade; a dump of the deployed program matched it after, with no
  non-zero padding.
- Before the upgrade, 1 input token was donated to the escrow of an old Passed
  commitment (`2UmAP7rZ...`, tx
  `2LbX5LRuVfyWCPLgkRxJRNADzpKkYuhbQfKjckq79S6noaNNCK1mXeKoU1JgGKG66Ez4d1N5wUgYZhCfYp6pbFG3`).
  `release.ts` in its legacy 7-account mode simulated against the deployed Slice
  2 binary and failed at the escrow close, `Custom(11)` (0xb); nothing was sent.
- After the upgrade, the same commitment was released (slot 508283971, tx
  `NQ16U67mCyRjnomJWW5AoaMXWkoa3DFFHACE78qmrabVYnJEVc6oXUcGgNDHYsYw8PpYE14i4Wec8YXPDP6mFK6`,
  Finalized): escrow and output accounts closed, status Released, payer input
  1149999 to 1150000 (the donated unit), payer output 300000 to 1300000.
- Run D (`5zrxT6Ad...`) was then released (slot 508284268, tx
  `2kU4tMx6yU9LGWosVZiXhZ3SiuAq5NvmvBvD99m1FUYMRdXXbYdKdLk5i9YVeKCXJEqrTPx2NWo8xe4bX7a4WrQR`,
  Finalized): escrow was already 0, payer input unchanged at 1150000, payer
  output 1300000 to 2300000, status Released.
- The swap in these runs is the disclosed `mock_swap` stand-in, not a real
  venue. Fifteen older Passed commitments remain unreleased. `refund` ran once on
  devnet, on a commitment that never reached the swap (see "Refund run"
  below). The fee is a design only and is not implemented.

### Refund run (devnet, Oct 7)

- This is a refund on a commitment that never reached the swap, not the normal
  flow: the normal worker flow withdraws the escrow first, so those runs exit
  through `recover`. It ran against the deployed Slice 3 program.
- Commitment `FWaLntwjzeroq3FFji7gReXeqM7Wsq1vTf5KgwZWq5DU`, input 1000000,
  minimum output 500000. `create_commitment` tx
  `67N3fxP7prYG2Bjs4rHcPYr8CLAngiCm1VhkEvYqXjcFtXTQRPH5wEK6yLPBUmCSzewMv9PVRffGoHJAEUXobrcE`.
  `withdraw_for_swap` was not called. `submit_proof` tx
  `5KaYbG1SYaCrBagtb6k2RGwe7jKMinPhozwbnwavLfjqVgDedPzN32e8EGKYW8zVJVuAMurdq6WdEagXDYkJcaYp`
  claimed 0 against an empty output account, so the status became
  FailedSlippage with `escrow_withdrawn` false.
- `refund` (`worker/src/refund.ts`, commit `ea0e445`), signed by the main
  wallet, tx
  `3JdMXHRsTJoY7DfthGbdcZhpkDJpzPqcSHnE8q63ky4dJBJDkszJyDzftfUvFVzbk8ANUuVmwBqt7ZHS74Xy9iJc`,
  slot 508392421, Finalized. Before: escrow 1000000, output 0, payer input
  1150000, payer output 2300000. After: escrow and output accounts closed,
  status Refunded, payer input 2150000, payer output 2300000.
- A second run of the script refused (status Refunded) and sent nothing. The
  program-level second-refund error (3012, AccountNotInitialized on the closed
  escrow) was observed in litesvm only.

### Limits of the evidence

- The litesvm suite (34 tests) sets some state directly. Only the devnet runs
  show real chain behaviour.
- `cancel` is exercised in litesvm test B7 and in devnet Run C2. `release` is
  covered by three litesvm tests (a no-dust baseline, the dust variant and the
  output-donation variant) and by two documented devnet releases (Slice 3
  above). `refund` is covered by two litesvm success tests (output 0, and
  output non-zero but below the minimum; a second refund fails with
  AccountNotInitialized on the closed escrow) and one test that shows it
  returns WrongState on a Passed commitment. `refund` ran once on devnet, on a
  commitment that never reached the swap (see "Refund run" above). `refund` is reachable only for FailedSlippage with the
  escrow never withdrawn; the normal worker flow withdraws first, so those
  runs exit through `recover`.
- `recover.ts` and `cancel.ts` refuse a settler equal to the payer. That is a
  script choice, not a program rule.
- Slot rate was measured at 4.269 slots/s in one 104 s window on Oct 5, not a
  constant.
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
