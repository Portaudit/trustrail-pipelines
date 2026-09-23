# TrustRail

Deterministic settlement, no judge model — the chain is the verdict.

TrustRail is an on-chain outcome-verification and settlement layer for AI
agent-to-agent commerce on Solana, built for the Colosseum Crypto World's Fair
hackathon. An agent locks funds against a commitment (a task spec with a minimum
acceptable output amount and a deadline), a swap is executed, and the commitment's
critical path — verifying the outcome and releasing or refunding funds — is decided
entirely by on-chain spec-matching. No LLM or semantic judgment is ever part of the
settlement decision.

## Why "no judge model"

A prior project, AuditAgent, used a semantic/LLM audit gate to judge task outcomes.
TrustRail replaces that with deterministic on-chain comparison: a commitment
specifies a `min_output_amount`, the actual output is measured on-chain, and
`submit_proof` compares the two directly. There is no model, no prompt, and no
subjective judgment anywhere in the settlement path.

## Programs

| Program | Program ID (devnet) |
|---|---|
| `trustrail_pipelines` | `CgXidrtsV5nLkjPCYhZUPUuZUpmvekMKvDN9uh3ucoC8` |
| `mock_swap` | `3qqN3ZXc1SPEtDLdn9Ek68r9YtDc1mBVgdhS7r48d9vJ` |

### `trustrail_pipelines`

The core settlement program. Instructions: `create_commitment`,
`withdraw_for_swap`, `submit_proof`, `release`, `refund`, `cancel`.

### `mock_swap`

A stand-in for a real swap venue (e.g. Jupiter), built to exercise a real,
on-chain, non-trivial swap step in the pipeline before a production venue
integration exists. It performs real token transfers (not minting-into-existence)
at a fixed 1:1 rate. See [`docs/KNOWN_LIMITATIONS.md`](docs/KNOWN_LIMITATIONS.md)
for the full disclosure of what this program does and does not simulate.

## Settlement flow

```
create_commitment -> withdraw_for_swap -> [swap venue] -> submit_proof -> release / refund
```

1. `create_commitment` — locks input tokens into an escrow ATA, records
   `min_output_amount`, a deadline slot, and (as of the escrow-withdrawal safety
   layer) an `expected_staging_ata` pinned at creation time.
2. `withdraw_for_swap` — moves the escrowed input from `escrow_ata` into the
   pinned `swap_staging_ata`, and only that exact address; any other destination
   is rejected (`StagingAtaMismatch`).
3. A swap executes against `swap_staging_ata`, producing output in `output_ata`
   (currently via `mock_swap_execute`; swappable for a real venue without
   changing any TrustRail settlement logic).
4. `submit_proof` — compares the real `output_ata` balance against
   `min_output_amount` and sets the commitment's status to `Passed` or
   `FailedSlippage` accordingly, purely from on-chain state.
5. `release` or `refund` follows from that status.

## Running the demo (devnet)

From `worker/`:

```bash
npm run create-commitment      # locks escrow, creates a per-commitment staging ATA
npm run withdraw-for-swap      # moves escrow -> staging (guarded, pinned destination)
npm run execute-mock-swap      # staging -> pool -> output_ata (real transfers)
npm run submit-proof           # compares output_ata against min_output_amount on-chain
npm run get-commitment -- <COMMITMENT_PDA>   # independently decode the account
```

`npm run initialize-mock-pool` is one-time setup for the mock swap pool's
liquidity and only needs to be run once per fixed mint pair (see
[`docs/KNOWN_LIMITATIONS.md`](docs/KNOWN_LIMITATIONS.md) for why mints are fixed
rather than freshly minted per run).

## Verified example runs (devnet)

**Passed** (task `a7c709942f0542765cea54444823f7d4a5606d62575b4a71de02209a08deb96c`,
commitment [`DeoSo4Dy3hQ8R9GgpgeXdXNgAYkcy6nyFRbThDGUB3Ur`](https://explorer.solana.com/address/DeoSo4Dy3hQ8R9GgpgeXdXNgAYkcy6nyFRbThDGUB3Ur?cluster=devnet)):

- [`create_commitment`](https://explorer.solana.com/tx/4PzKMc8cZwgAH9umySMaCjJQKLjXs5ooeW7C1nLd93Mntm4ahtyCC46MwDtGYDYpSvTgcM9bDnv8PnBu8o7ivHPs?cluster=devnet)
- [`withdraw_for_swap`](https://explorer.solana.com/tx/3WJGHca2GJcjsBMBHQmN1SHhBEJsDWK9kfFNesX8HcPQDDGJzMsLQWDyAm1Se1MKi3s3NEj4pm1yy6VQ7cTcUKAF?cluster=devnet)
- [`mock_swap_execute`](https://explorer.solana.com/tx/3zFdaa8PfGFMVg6FCnwt6ULH9jifzcT8vrtAxpNaPtQqYuWfT455qCgTmToJMRwmZuWjtMhu3ZjUznor1HqXBhJS?cluster=devnet)
- [`submit_proof`](https://explorer.solana.com/tx/2DP3LxG8J4Lymy7fEvpxcVGn3DRJV1u3tXEEywPGFb3aquVXMcoutkd7F19PjCHH3UPfuetdFScRNN2DRzLJG1RF?cluster=devnet) — independently decoded on-chain status: `Passed`

**FailedSlippage** (task `b66b8e4b7f479828d25d8b2c4cf1e8f9352c1953b978f03fca7433845ca82ad9`,
commitment [`HAi7f8Qf5AsJCSre5ruqMZQ6RchLG6TTuUPKogHMyiPA`](https://explorer.solana.com/address/HAi7f8Qf5AsJCSre5ruqMZQ6RchLG6TTuUPKogHMyiPA?cluster=devnet)):

- [`create_commitment`](https://explorer.solana.com/tx/3FEYZcuHTvzp9dWYCcXbhrUHr2k7J4MFEQ1HJhwYDi3GGeJt6P4H169L8Mnqhe5iPuA9PikPHj8j7EGWsTMHh3x9?cluster=devnet)
- [`withdraw_for_swap`](https://explorer.solana.com/tx/3RvEgZnhqDK2ozPoNBeA6pE9uZ8v7hafyGi7kPmjd8QJC1CwLiF4zFWTfnr11crxXMoC7MdCWAKnWjneMEN34XSs?cluster=devnet)
- [`mock_swap_execute`](https://explorer.solana.com/tx/dDGNfRD2Ecate3xj2qjyo79bK4BFgerLxSHrLpWDtLPxmizGgR9iL8gpRcZc75nFWri3M7ortM6uwibZwPtFWne?cluster=devnet)
- [`submit_proof`](https://explorer.solana.com/tx/3zHeaUESpu4LZ235MKTF5N3SC5DtfVX5SgVQypezJDbmt9hebXS1HAST6gEWbm2v625Zwx2mmevKeSzZkj13b6yo?cluster=devnet) — independently decoded on-chain status: `FailedSlippage`

## Docs

- [`docs/KNOWN_LIMITATIONS.md`](docs/KNOWN_LIMITATIONS.md) — honest disclosures:
  the mock swap venue's scope, a stuck-funds gap after `withdraw_for_swap`, and
  other known gaps.
- [`docs/SCHEMA.md`](docs/SCHEMA.md) — account layouts.
