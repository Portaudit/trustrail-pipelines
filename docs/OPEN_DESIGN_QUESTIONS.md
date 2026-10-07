# Open design questions

Questions that are decided for now but need a deliberate answer later. Each has a trigger, so it gets revisited at the right moment and not by memory.

## Q1. Licensing and placement of future proprietary components

- **Status:** open. Recorded 2026-10-07.
- **Current decision:** everything in this repository, program and worker, carries `// Copyright 2026 Ishvir and Company (Pty) Ltd` and `// SPDX-License-Identifier: Apache-2.0`. This holds through the Colosseum submission.
- **Question:** if Ishvir and Company later builds components that should not be open (fee logic, a hosted worker, customer integrations), where do they live and under what terms?
- **Default answer:** a separate private repository from the first commit. Apache-2.0 files in this repository must not copy in private code. Private files use a copyright line with "All rights reserved" and no SPDX tag.
- **Trigger:** the first line of such a component is written. Create the private repository before that line, not after.
- **Why not now:** the fee design is spec-only and nothing proprietary exists in code. The worker is thin glue over on-chain state, and keeping it open helps the verifiability claim.
- **Cost of getting it wrong:** code that was ever pushed here stays in git history even if later removed, so moving it later does not make it private.

Copyright 2026 Ishvir and Company (Pty) Ltd.
