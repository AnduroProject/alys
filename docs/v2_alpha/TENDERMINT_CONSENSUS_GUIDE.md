# Tendermint Consensus: A Complete Guide

## Part 1: The Problem We're Solving

### What is Consensus?

Imagine you have 5 friends who need to agree on where to eat dinner. Everyone can suggest a restaurant, but you need a system where:
1. **Everyone agrees** on the same restaurant (no splitting up)
2. **The decision is final** (no changing minds after agreeing)
3. **It works even if 1 friend is being difficult** (fault tolerance)

This is the consensus problem. In blockchain terms:
- "Friends" = Validator nodes
- "Restaurant" = The next block to add to the chain
- "Being difficult" = A node crashing, having network issues, or actively lying

### Why is This Hard?

```
Node A thinks: "Let's add Block X"
Node B thinks: "Let's add Block Y"
Node C thinks: "Let's add Block X"
Node D is offline...
Node E thinks: "Let's add Block Y"
```

Without a protocol, you get chaos - different nodes build different chains, and the network "forks."

### Tendermint's Promise

Tendermint guarantees:
1. **Safety**: All honest nodes agree on the same block (no forks)
2. **Liveness**: The network keeps making progress (blocks keep being added)
3. **Byzantine Fault Tolerance**: Works correctly even if up to 1/3 of validators are malicious

---

## Part 2: Core Concepts

### Validators

Validators are the nodes that participate in consensus. Each validator:
- Has a **public/private key pair** for signing messages
- Has **voting power** (often based on stake)
- Can **propose blocks** and **vote** on proposals

```
┌─────────────────────────────────────────────────────────┐
│                    VALIDATOR SET                        │
├─────────────┬─────────────┬─────────────┬──────────────┤
│ Validator A │ Validator B │ Validator C │ Validator D  │
│ Power: 25%  │ Power: 25%  │ Power: 25%  │ Power: 25%   │
└─────────────┴─────────────┴─────────────┴──────────────┘
                    Total Voting Power: 100%

         Quorum (2/3+) needed to decide: 67%+
```

### Heights and Rounds

**Height**: Which block number we're trying to agree on.
```
Height 0: Genesis block (pre-agreed)
Height 1: First block to decide
Height 2: Second block to decide
...
```

**Round**: Attempt number within a height. If consensus fails (timeout, bad proposer), we move to the next round.
```
Height 5, Round 0: First attempt to decide block 5
Height 5, Round 1: Second attempt (if round 0 failed)
Height 5, Round 2: Third attempt (if round 1 failed)
...
```

### The Two Phases: Prevote and Precommit

Tendermint uses two voting phases to ensure safety:

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   PROPOSE    │ ──→ │   PREVOTE    │ ──→ │  PRECOMMIT   │
│              │     │              │     │              │
│ Leader sends │     │ "I've seen   │     │ "I'm ready   │
│ a block      │     │ this block"  │     │ to commit"   │
└──────────────┘     └──────────────┘     └──────────────┘
```

**Why two phases?** One phase isn't enough to guarantee safety under network delays. The two-phase approach ensures that once a block *could* be committed, everyone knows about it.

---

## Part 3: The Protocol Step-by-Step

Let's walk through one complete round with 4 validators (A, B, C, D).

### Step 1: Propose

The **proposer** (selected round-robin) broadcasts a block proposal.

```
Round 0 Proposer: Validator A (determined by: height + round mod num_validators)

        ┌───────────────┐
        │  Validator A  │
        │   (Proposer)  │
        └───────┬───────┘
                │
                │  PROPOSE(Block #5, Round 0)
                │  "Here's the block I want to add"
                ▼
    ┌───────────────────────────────────┐
    │         BROADCAST TO ALL          │
    └───────────────────────────────────┘
                │
       ┌────────┼────────┐
       ▼        ▼        ▼
    ┌─────┐  ┌─────┐  ┌─────┐
    │  B  │  │  C  │  │  D  │
    └─────┘  └─────┘  └─────┘
```

The proposal contains:
```rust
struct Proposal {
    height: u64,           // Block height (e.g., 5)
    round: u32,            // Round number (e.g., 0)
    block: Block,          // The actual block data
    proposer_signature: Signature,
}
```

### Step 2: Prevote

Each validator **validates** the proposal and broadcasts a PREVOTE.

```
Each validator checks:
  ✓ Is the block valid? (transactions, hash, etc.)
  ✓ Does it build on the correct parent?
  ✓ Is it from the legitimate proposer for this round?

If YES → Prevote for the block
If NO  → Prevote NIL (empty vote)
```

```
        ┌─────┐  ┌─────┐  ┌─────┐  ┌─────┐
        │  A  │  │  B  │  │  C  │  │  D  │
        └──┬──┘  └──┬──┘  └──┬──┘  └──┬──┘
           │       │       │       │
           ▼       ▼       ▼       ▼
    ┌────────────────────────────────────────┐
    │          PREVOTE BROADCAST             │
    │                                        │
    │  A: PREVOTE(Block#5) ──────────────→  │
    │  B: PREVOTE(Block#5) ──────────────→  │
    │  C: PREVOTE(Block#5) ──────────────→  │
    │  D: PREVOTE(Block#5) ──────────────→  │
    │                                        │
    │         All validators see ALL         │
    │         prevotes (all-to-all)          │
    └────────────────────────────────────────┘
```

**Prevote message**:
```rust
struct Prevote {
    height: u64,
    round: u32,
    block_hash: Option<Hash>,  // Some(hash) or None for NIL
    validator_signature: Signature,
}
```

### Step 3: Wait for 2/3+ Prevotes

Each validator waits until they see **2/3+ prevotes for the same block** (or 2/3+ for anything, including NIL).

```
Validator B's view after collecting prevotes:

┌─────────────────────────────────────────┐
│         PREVOTES RECEIVED               │
├─────────────────────────────────────────┤
│  From A: PREVOTE(Block#5) ✓            │
│  From B: PREVOTE(Block#5) ✓ (own vote) │
│  From C: PREVOTE(Block#5) ✓            │
│  From D: PREVOTE(Block#5) ✓            │
├─────────────────────────────────────────┤
│  Total for Block#5: 4/4 = 100%         │
│  Threshold needed:  2/3 = 67%          │
│  RESULT: Got 2/3+ prevotes! ✓          │
└─────────────────────────────────────────┘
```

### Step 4: Precommit

Once a validator sees 2/3+ prevotes for a block, they broadcast a PRECOMMIT.

```
        ┌─────┐  ┌─────┐  ┌─────┐  ┌─────┐
        │  A  │  │  B  │  │  C  │  │  D  │
        └──┬──┘  └──┬──┘  └──┬──┘  └──┬──┘
           │       │       │       │
           │  (Each saw 2/3+ prevotes)
           │       │       │       │
           ▼       ▼       ▼       ▼
    ┌────────────────────────────────────────┐
    │         PRECOMMIT BROADCAST            │
    │                                        │
    │  A: PRECOMMIT(Block#5) ────────────→  │
    │  B: PRECOMMIT(Block#5) ────────────→  │
    │  C: PRECOMMIT(Block#5) ────────────→  │
    │  D: PRECOMMIT(Block#5) ────────────→  │
    └────────────────────────────────────────┘
```

### Step 5: Commit

Once a validator sees **2/3+ precommits** for a block, the block is **committed**.

```
┌─────────────────────────────────────────────────────────┐
│                      COMMIT!                            │
│                                                         │
│   Block #5 is now FINAL and added to the chain         │
│                                                         │
│   ┌─────┐   ┌─────┐   ┌─────┐   ┌─────┐   ┌─────┐     │
│   │  0  │──▶│  1  │──▶│  2  │──▶│  3  │──▶│  4  │──▶  │
│   └─────┘   └─────┘   └─────┘   └─────┘   └─────┘     │
│                                                 │       │
│                                                 ▼       │
│                                            ┌─────┐     │
│                                            │  5  │ NEW │
│                                            └─────┘     │
└─────────────────────────────────────────────────────────┘

All validators now move to Height 6, Round 0
```

---

## Part 4: Complete Timeline Diagram

Here's the full protocol for one successful round:

```
TIME ──────────────────────────────────────────────────────────────────▶

         │ PROPOSE │     PREVOTE      │    PRECOMMIT     │   COMMIT
         │  PHASE  │      PHASE       │      PHASE       │   PHASE
         │         │                  │                  │
    A ───┼────●────┼────●─────────────┼────●─────────────┼────●────────
         │    │    │    │             │    │             │    │
         │ Propose │  Prevote         │ Precommit        │  Commit
         │ Block#5 │  Block#5         │ Block#5          │  Block#5
         │         │    │             │    │             │    │
    B ───┼─────────┼────●─────────────┼────●─────────────┼────●────────
         │         │    │             │    │             │    │
         │ Receive │  Prevote         │ Precommit        │  Commit
         │ proposal│  Block#5         │ Block#5          │  Block#5
         │         │    │             │    │             │    │
    C ───┼─────────┼────●─────────────┼────●─────────────┼────●────────
         │         │    │             │    │             │    │
         │ Receive │  Prevote         │ Precommit        │  Commit
         │ proposal│  Block#5         │ Block#5          │  Block#5
         │         │    │             │    │             │    │
    D ───┼─────────┼────●─────────────┼────●─────────────┼────●────────
         │         │    │             │    │             │    │
         │ Receive │  Prevote         │ Precommit        │  Commit
         │ proposal│  Block#5         │ Block#5          │  Block#5
         │         │                  │                  │
         │         │  Wait for 2/3+   │  Wait for 2/3+   │
         │         │  prevotes        │  precommits      │
```

---

## Part 5: When Things Go Wrong

### Scenario 1: Proposer is Offline

```
Height 5, Round 0:
  - Validator A is the proposer
  - A is offline/crashed
  - No proposal arrives

        ┌───────────────┐
        │  Validator A  │
        │   (OFFLINE)   │  ← No proposal sent!
        └───────────────┘

    B, C, D wait... wait... wait...

    TIMEOUT! (e.g., 3 seconds)

    All validators prevote NIL:

    B: PREVOTE(NIL)
    C: PREVOTE(NIL)
    D: PREVOTE(NIL)

    Collect 2/3+ NIL prevotes → Precommit NIL
    Collect 2/3+ NIL precommits → Move to Round 1

Height 5, Round 1:
  - Validator B is now the proposer
  - B proposes Block#5
  - Normal protocol continues...
```

### Scenario 2: Invalid Block Proposed

```
Height 5, Round 0:
  - Validator A proposes an INVALID block
  - (e.g., invalid transactions, wrong parent hash)

        ┌───────────────┐
        │  Validator A  │
        │  (Malicious)  │
        └───────┬───────┘
                │
                │ PROPOSE(Invalid Block)
                ▼
    ┌─────┐  ┌─────┐  ┌─────┐
    │  B  │  │  C  │  │  D  │
    └──┬──┘  └──┬──┘  └──┬──┘
       │       │       │
    Validate  Validate  Validate
       │       │       │
     FAIL!    FAIL!    FAIL!
       │       │       │
       ▼       ▼       ▼

    B: PREVOTE(NIL)  "I reject this block"
    C: PREVOTE(NIL)  "I reject this block"
    D: PREVOTE(NIL)  "I reject this block"

    2/3+ NIL prevotes → Round fails → Move to Round 1
```

### Scenario 3: Network Partition (The Tricky Case)

```
Network splits! A and B can talk. C and D can talk. But the groups can't reach each other.

    ┌─────────────────┐          ┌─────────────────┐
    │   Partition 1   │          │   Partition 2   │
    │                 │    ✗     │                 │
    │   A ←──→ B      │◄──────►  │   C ←──→ D      │
    │   (50%)         │  SPLIT   │   (50%)         │
    └─────────────────┘          └─────────────────┘

Neither partition has 2/3+ (67%) of voting power!

Partition 1:
  - A proposes Block X
  - A and B prevote for X
  - Only 50% prevotes → Can't reach 2/3+ → STUCK

Partition 2:
  - C and D wait for proposal (never arrives from A)
  - Timeout → Prevote NIL
  - Only 50% prevotes → STUCK

RESULT: Network halts until partition heals
        (This is SAFETY over LIVENESS - no bad blocks committed)
```

This is a fundamental trade-off: Tendermint chooses **safety** (never commit conflicting blocks) over **liveness** (always make progress). During a severe partition, the network stops rather than risk a fork.

---

## Part 6: The Locking Mechanism (Critical for Safety)

The **locking rule** is what makes Tendermint safe. Without it, you could have this disaster:

### The Problem Without Locking

```
Imagine this scenario without locking:

Round 0:
  - A proposes Block X
  - A, B, C prevote X (3/4 = 75% ✓)
  - A, B precommit X (only 2/4 = 50%, need more)
  - C crashes before precommitting!
  - D prevoted NIL (didn't see proposal in time)
  - Round times out, move to Round 1

Round 1:
  - B proposes Block Y (different block!)
  - Everyone prevotes Y
  - Everyone precommits Y
  - Y is committed!

BUT WAIT: A and B had precommitted X in Round 0!
If they committed X and then Y was also committed elsewhere...
DISASTER: Two different blocks at the same height!
```

### The Locking Rule (Solution)

**Rule**: Once you precommit a block, you are "locked" on it. You cannot prevote for a different block in future rounds.

```rust
struct ValidatorState {
    locked_round: Option<u32>,      // Round when we got locked
    locked_block: Option<BlockHash>, // Block we're locked on
}

fn can_prevote(&self, block: &Block, round: u32) -> bool {
    match (self.locked_round, self.locked_block) {
        // Not locked → can prevote anything
        (None, _) => true,

        // Locked → can only prevote our locked block
        // UNLESS we see 2/3+ prevotes for another block in a LATER round
        (Some(locked_round), Some(locked_hash)) => {
            if block.hash() == locked_hash {
                true  // Always OK to vote for locked block
            } else if round > locked_round && saw_2_3_prevotes_for(block) {
                true  // Can unlock if proof exists
            } else {
                false // Must stay loyal to locked block
            }
        }
    }
}
```

### Locking Example

```
Round 0:
  ┌─────────────────────────────────────────────────────────┐
  │ A proposes Block X                                      │
  │ A, B, C, D all prevote X  (4/4 = 100%)                 │
  │ A, B, C precommit X       (3/4 = 75%)                  │
  │ D's network is slow, times out                          │
  │                                                         │
  │ A, B, C are now LOCKED on Block X                      │
  │ ┌─────────────────────────────────────────────────────┐│
  │ │ A.locked_block = X    A.locked_round = 0           ││
  │ │ B.locked_block = X    B.locked_round = 0           ││
  │ │ C.locked_block = X    C.locked_round = 0           ││
  │ │ D.locked_block = None                               ││
  │ └─────────────────────────────────────────────────────┘│
  │                                                         │
  │ Round times out (didn't get 2/3+ precommits fast enough)│
  └─────────────────────────────────────────────────────────┘

Round 1:
  ┌─────────────────────────────────────────────────────────┐
  │ D proposes Block Y (different from X!)                  │
  │                                                         │
  │ Can validators prevote Y?                               │
  │                                                         │
  │ A: LOCKED on X → prevotes X (ignores Y proposal)       │
  │ B: LOCKED on X → prevotes X                            │
  │ C: LOCKED on X → prevotes X                            │
  │ D: Not locked → prevotes Y (their own proposal)        │
  │                                                         │
  │ Prevote results: X=3, Y=1                              │
  │ 2/3+ prevotes for X → precommit X                      │
  │ 2/3+ precommits for X → COMMIT X!                      │
  │                                                         │
  │ Block X is committed (same block that was almost       │
  │ committed in Round 0 - SAFETY PRESERVED!)              │
  └─────────────────────────────────────────────────────────┘
```

### Unlocking (When You Can Change Your Lock)

You can only unlock if you see **proof** (2/3+ prevotes in a later round) that the network has moved on:

```
Round 0:
  - A locks on Block X (saw 2/3+ prevotes, precommitted)

Round 1:
  - A sees 2/3+ prevotes for Block Y in Round 1
  - Since Round 1 > Round 0 (A's locked round), A can unlock
  - A updates: locked_block = Y, locked_round = 1
  - A can now prevote/precommit Y
```

This "Proof of Lock Change" (PoLC) ensures safety while allowing progress.

---

## Part 7: End-to-End Example with All Details

Let's trace through a complete realistic scenario with 4 validators.

### Setup

```
Validators: A, B, C, D (each with 25% voting power)
Current chain: [Genesis] → [Block 1] → [Block 2] → [Block 3] → [Block 4]
Goal: Decide Block 5

Proposer schedule (round-robin):
  Round 0: A
  Round 1: B
  Round 2: C
  Round 3: D
  ...
```

### Height 5, Round 0

**T = 0s: Propose Phase**
```
A is the proposer for Round 0.

A creates a block:
┌────────────────────────────────────────┐
│ Block #5                               │
├────────────────────────────────────────┤
│ parent_hash: hash(Block#4)             │
│ transactions: [tx1, tx2, tx3]          │
│ timestamp: 2026-01-21 10:00:00         │
│ proposer: A                            │
└────────────────────────────────────────┘

A broadcasts: PROPOSE(height=5, round=0, block=Block#5)
```

**T = 0.1s: All validators receive proposal**
```
B receives proposal → validates block → VALID
C receives proposal → validates block → VALID
D receives proposal → validates block → VALID
```

**T = 0.2s: Prevote Phase**
```
All validators broadcast prevotes:

A → PREVOTE(height=5, round=0, block_hash=hash(Block#5), sig_A)
B → PREVOTE(height=5, round=0, block_hash=hash(Block#5), sig_B)
C → PREVOTE(height=5, round=0, block_hash=hash(Block#5), sig_C)
D → PREVOTE(height=5, round=0, block_hash=hash(Block#5), sig_D)

Message count: 4 validators × 4 recipients = 16 messages (O(n²))
```

**T = 0.5s: Prevotes collected**
```
Each validator's view:

┌──────────────────────────────────────────────────┐
│ Validator A's Prevote Collection                 │
├──────────────────────────────────────────────────┤
│ From A: PREVOTE(Block#5) ✓                      │
│ From B: PREVOTE(Block#5) ✓                      │
│ From C: PREVOTE(Block#5) ✓                      │
│ From D: PREVOTE(Block#5) ✓                      │
├──────────────────────────────────────────────────┤
│ Tally: 4/4 (100%) for Block#5                   │
│ Threshold: 2/3+ (67%)                           │
│ PASSED! → Proceed to precommit                   │
└──────────────────────────────────────────────────┘
```

**T = 0.6s: Precommit Phase**
```
All validators see 2/3+ prevotes, so they precommit:

A → PRECOMMIT(height=5, round=0, block_hash=hash(Block#5), sig_A)
B → PRECOMMIT(height=5, round=0, block_hash=hash(Block#5), sig_B)
C → PRECOMMIT(height=5, round=0, block_hash=hash(Block#5), sig_C)
D → PRECOMMIT(height=5, round=0, block_hash=hash(Block#5), sig_D)

All validators are now LOCKED on Block#5, Round 0

Message count: 4 × 4 = 16 more messages
```

**T = 1.0s: Precommits collected**
```
Each validator's view:

┌──────────────────────────────────────────────────┐
│ Validator B's Precommit Collection               │
├──────────────────────────────────────────────────┤
│ From A: PRECOMMIT(Block#5) ✓                    │
│ From B: PRECOMMIT(Block#5) ✓                    │
│ From C: PRECOMMIT(Block#5) ✓                    │
│ From D: PRECOMMIT(Block#5) ✓                    │
├──────────────────────────────────────────────────┤
│ Tally: 4/4 (100%) for Block#5                   │
│ Threshold: 2/3+ (67%)                           │
│ PASSED! → COMMIT BLOCK!                          │
└──────────────────────────────────────────────────┘
```

**T = 1.1s: Commit**
```
All validators commit Block#5 to their chain:

BEFORE:
[Genesis] → [Block 1] → [Block 2] → [Block 3] → [Block 4]

AFTER:
[Genesis] → [Block 1] → [Block 2] → [Block 3] → [Block 4] → [Block 5] ✓

Block#5 is now FINAL. It cannot be reverted.

All validators:
  - Clear their locks
  - Move to Height 6, Round 0
  - Wait for new proposer (B for Height 6, Round 0)
```

### Total Messages for Height 5

```
Phase        Messages
─────────────────────
Propose      4  (1 proposer → 4 validators)
Prevote      16 (4 validators × 4 recipients)
Precommit    16 (4 validators × 4 recipients)
─────────────────────
Total        36 messages

Formula: O(n²) = O(4²) = O(16) per voting phase
```

---

## Part 8: State Machine Summary

Each validator runs this state machine:

```
                              ┌─────────────────┐
                              │    NEW ROUND    │
                              │  (Enter Round)  │
                              └────────┬────────┘
                                       │
                                       ▼
                              ┌─────────────────┐
                              │    PROPOSE      │
              ┌───────────────│  Wait for or    │
              │               │  send proposal  │
              │               └────────┬────────┘
              │                        │
              │  Timeout               │ Received valid proposal
              │  (no proposal)         │ OR timeout
              │                        ▼
              │               ┌─────────────────┐
              │               │    PREVOTE      │
              │               │  Cast prevote   │───────────────┐
              │               │  (block or NIL) │               │
              │               └────────┬────────┘               │
              │                        │                        │
              │                        │ Got 2/3+ prevotes      │
              │                        │ (for block or NIL)     │
              │                        ▼                        │
              │               ┌─────────────────┐               │
              │               │   PRECOMMIT     │               │
              │               │ Cast precommit  │               │
              │               │ (block or NIL)  │               │
              │               └────────┬────────┘               │
              │                        │                        │
              │         ┌──────────────┴──────────────┐        │
              │         │                             │        │
              │         ▼                             ▼        │
              │  ┌─────────────┐              ┌─────────────┐  │
              │  │ 2/3+ for    │              │ 2/3+ for    │  │
              │  │ a BLOCK     │              │ NIL or      │  │
              │  │             │              │ timeout     │  │
              │  └──────┬──────┘              └──────┬──────┘  │
              │         │                           │          │
              │         ▼                           │          │
              │  ┌─────────────┐                    │          │
              │  │   COMMIT    │                    │          │
              │  │ Add block   │                    │          │
              │  │ to chain    │                    │          │
              │  └──────┬──────┘                    │          │
              │         │                           │          │
              │         ▼                           ▼          │
              │  ┌─────────────┐           ┌─────────────┐     │
              │  │ NEW HEIGHT  │           │  NEW ROUND  │◄────┘
              │  │ height + 1  │           │  round + 1  │
              │  │ round = 0   │           │ same height │
              │  └─────────────┘           └─────────────┘
              │         │                           ▲
              └─────────┴───────────────────────────┘
                        (if commit happens, go to new height)
```

---

## Part 9: Key Takeaways

### The Two-Phase Voting Explained Simply

1. **Prevote** = "I've seen this block and it looks valid"
2. **Precommit** = "I've seen that 2/3+ of us agree it looks valid, so I'm committing to it"

The second phase is necessary because of network delays - just because *you* saw 2/3+ prevotes doesn't mean everyone did. The precommit phase ensures everyone knows that a quorum exists.

### Why 2/3+ Threshold?

With 2/3+ (67%), even if 1/3 of validators are Byzantine (malicious), honest validators (2/3+) can still reach consensus:

```
Validators: 100 total
Byzantine:  33 (worst case)
Honest:     67

For any two 2/3+ groups, they MUST overlap by at least 1/3+:
  Group 1: 67 validators
  Group 2: 67 validators
  Total:   100 validators

  Overlap = 67 + 67 - 100 = 34 validators

This overlap ensures any two committed values must be the same
(the overlapping honest validators won't vote for conflicting blocks)
```

### Tendermint Properties Summary

| Property | Guarantee |
|----------|-----------|
| **Safety** | No two honest validators commit different blocks at the same height |
| **Liveness** | If 2/3+ validators are honest and can communicate, blocks will be committed |
| **Finality** | Once committed, a block is final (no reorganizations) |
| **Fault tolerance** | Tolerates up to 1/3 Byzantine (malicious) validators |

---

This covers the core of Tendermint consensus. The actual implementation includes additional optimizations (proposal caching, evidence handling, validator set changes), but the fundamental protocol is what I've described above.

---

## Part 10: Deep Dive into Proof-of-Lock (POL)

Part 6 introduced locking, but this section provides a more thorough explanation of why POL exists and exactly how it prevents forks.

### The Core Problem POL Solves

Consider this scenario without POL:

```
Height 100, Round 0:
  - Proposer A proposes Block X
  - Validators collect 2/3+ prevotes for Block X
  - Validator V precommits for Block X (they're now "locked")
  - Network partition occurs - precommits don't reach quorum
  - Round times out, moves to Round 1

Height 100, Round 1:
  - Proposer B proposes Block Y (different block!)
  - Without POL: Validator V could prevote for Block Y
  - DANGER: If enough validators do this, both X and Y could
    get 2/3+ precommits across different rounds = FORK
```

### How Locking Works Step-by-Step

A validator becomes **locked** on a block when they see 2/3+ prevotes for it:

```
┌─────────────────────────────────────────────────────────────────┐
│  Validator V at Height 100                                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Round 0:                                                       │
│    1. Sees proposal for Block X                                 │
│    2. Prevotes for Block X                                      │
│    3. Sees 2/3+ prevotes for Block X                            │
│    4. ──► LOCKED on Block X at round 0 ◄──                      │
│    5. Precommits for Block X                                    │
│    6. Round times out (not enough precommits)                   │
│                                                                 │
│  Round 1:                                                       │
│    - Sees proposal for Block Y                                  │
│    - CANNOT prevote for Y (locked on X)                         │
│    - Must prevote NIL or for X only                             │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### The Unlock Condition

A locked validator can only unlock if they see a **Proof-of-Lock** from a higher round:

```rust
// Simplified locking rule
fn determine_prevote(&self, proposal: &Proposal) -> Option<BlockHash> {
    match (&self.locked_block, &self.locked_round, proposal.pol_round) {
        // Not locked → vote for proposal
        (None, _, _) => Some(proposal.block_hash()),

        // Locked on THIS block → vote for it
        (Some(locked), _, _) if *locked == proposal.block_hash() => {
            Some(proposal.block_hash())
        }

        // Locked on different block, but proposal has POL from higher round
        // This proves 2/3+ validators prevoted for the new block in a later round
        (Some(_), Some(locked_round), Some(pol_round))
            if pol_round > locked_round => {
            // Safe to unlock - the network has moved on
            Some(proposal.block_hash())
        }

        // Locked on different block, no valid POL → vote NIL
        (Some(_), _, _) => None,
    }
}
```

### POL in the Proposal Message

When a proposer is locked, they MUST include `pol_round` in their proposal:

```rust
pub struct Proposal {
    pub height: Height,
    pub round: Round,
    pub block: Block,

    /// If proposer is locked, this indicates the round they locked
    /// Other validators use this to verify the proposer's lock is valid
    pub pol_round: Option<Round>,

    pub proposer: ValidatorId,
    pub signature: BLSSignature,
}
```

### Visual Timeline Example

```
Height 100 Timeline:
═══════════════════════════════════════════════════════════════════

Round 0:                          Round 1:
┌─────────────────────┐          ┌─────────────────────┐
│ Proposal: Block X   │          │ Proposal: Block X   │
│ pol_round: None     │          │ pol_round: 0  ←───── Proposer locked at R0
└─────────────────────┘          └─────────────────────┘
         │                                │
         ▼                                ▼
┌─────────────────────┐          ┌─────────────────────┐
│ Prevotes for X: 11  │          │ Validators check:   │
│ (2/3+ of 15)        │          │ - pol_round=0 valid │
│                     │          │ - Can vote for X    │
│ Validators LOCK     │          │                     │
│ on X at round 0     │          │                     │
└─────────────────────┘          └─────────────────────┘
         │                                │
         ▼                                ▼
┌─────────────────────┐          ┌─────────────────────┐
│ Precommits: 8       │          │ Precommits: 12      │
│ (not enough)        │          │ (2/3+ reached!)     │
│                     │          │                     │
│ Round timeout       │          │ BLOCK X COMMITTED   │
└─────────────────────┘          └─────────────────────┘
```

### Concrete 4-Validator Scenario

**Setup: 4 Validators (A, B, C, D)** - Need 3 votes (2/3+) to finalize.

#### Scenario WITHOUT Proof-of-Lock (Dangerous)

```
HEIGHT 100, ROUND 0:
════════════════════════════════════════════════════════════════

Proposer A proposes Block X

        A          B          C          D
        │          │          │          │
Prevote │────X────►│────X────►│────X────►│  (All prevote X)
        │          │          │          │
        │◄───X─────│◄───X─────│◄───X─────│
        │          │          │          │

Result: Everyone sees 3+ prevotes for X ✓

        A          B          C          D
        │          │          │          │
Precom- │────X────►│          │          │  A sends precommit
mit     │          │          │          │
        │    ╳ NETWORK PARTITION ╳       │  Messages lost!
        │          │          │          │

Result: Only A precommitted. Round times out. Move to Round 1.


HEIGHT 100, ROUND 1:
════════════════════════════════════════════════════════════════

Proposer B proposes Block Y (DIFFERENT block!)

WITHOUT LOCKING RULES:
        A          B          C          D
        │          │          │          │
Prevote │────Y────►│────Y────►│────Y────►│  Everyone prevotes Y
        │          │          │          │
Precom- │────Y────►│────Y────►│────Y────►│  Everyone precommits Y
        │          │          │          │

Result: Block Y is finalized with 4 precommits ✓

BUT WAIT - Validator A precommitted for X in Round 0!

If A's precommit from Round 0 eventually arrives at other nodes,
and they had also precommitted X... we could have TWO finalized blocks!

THIS IS A CATASTROPHIC FORK.
```

#### Same Scenario WITH Locking (Safe)

```
HEIGHT 100, ROUND 0:
════════════════════════════════════════════════════════════════

Proposer A proposes Block X

        A          B          C          D
        │          │          │          │
Prevote │────X────►│────X────►│────X────►│
        │◄───X─────│◄───X─────│◄───X─────│
        │          │          │          │

Everyone sees 3+ prevotes for X
        │          │          │          │
        ▼          ▼          ▼          ▼
     ┌──────┐  ┌──────┐  ┌──────┐  ┌──────┐
     │LOCKED│  │LOCKED│  │LOCKED│  │LOCKED│
     │on X  │  │on X  │  │on X  │  │on X  │
     │at R0 │  │at R0 │  │at R0 │  │at R0 │
     └──────┘  └──────┘  └──────┘  └──────┘

Round times out (precommits didn't reach quorum)


HEIGHT 100, ROUND 1:
════════════════════════════════════════════════════════════════

Proposer B wants to propose Block Y, BUT B is locked on X!

Option 1: B proposes X (the block they're locked on)
          ──► Include pol_round: 0 to prove they locked at round 0
          ──► Everyone else is ALSO locked on X, so they vote for it
          ──► Block X gets finalized ✓

Option 2: B proposes Y anyway (violating the rules)
          ──► Other validators see: "I'm locked on X, this proposal
              is for Y, and there's no POL proving the network moved on"
          ──► They vote NIL (refuse to vote for Y)
          ──► Y cannot get 2/3+ votes
          ──► Round times out, try again
```

### What is POL Actually Proving?

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  pol_round: 0  means:                                           │
│                                                                 │
│  "I (the proposer) saw 2/3+ prevotes for this block at round 0, │
│   which is why I'm proposing it. You can verify this is valid   │
│   because you should have also seen those prevotes."            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### When Can You Unlock?

You can ONLY unlock if you see proof that 2/3+ validators prevoted for a DIFFERENT block in a LATER round:

```
HEIGHT 100:

Round 0: You lock on X (saw 2/3+ prevotes for X)
Round 1: You're still locked on X
Round 2: You're still locked on X
Round 3: You see 2/3+ prevotes for Y (new POL at round 3)
         ──► You can now unlock from X and vote for Y
         ──► Because: if 2/3+ prevoted Y at round 3, the network
             has clearly moved on from X
```

### Jury Analogy

Think of it like a jury:

```
ROUND 0:
- Jury sees strong evidence for "Guilty" (2/3+ lean guilty)
- Each juror mentally commits: "I'm voting guilty unless
  I see compelling new evidence"
- But deliberation ends before final vote

ROUND 1:
- Someone proposes "Not Guilty"
- Locked jurors say: "No, I already saw strong evidence for Guilty.
  Show me that 2/3+ of us now believe Not Guilty, then I'll reconsider."
- Without that proof, they won't flip

This prevents:
- Some jurors finalizing "Guilty"
- Other jurors finalizing "Not Guilty"
- (Which would be chaos)
```

### The Key Safety Guarantee

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  IF block X gets 2/3+ precommits (finalized), THEN:             │
│                                                                 │
│  1. 2/3+ validators prevoted for X (required before precommit)  │
│  2. Those 2/3+ are now LOCKED on X                              │
│  3. A different block Y can NEVER get 2/3+ prevotes             │
│     (because >1/3 are locked on X and won't vote for Y)         │
│  4. Therefore Y can NEVER be finalized                          │
│                                                                 │
│  RESULT: Only ONE block can ever be finalized per height        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### POL Summary Table

| Concept | Purpose |
|---------|---------|
| **Lock** | Validator commits to a block after seeing 2/3+ prevotes |
| **POL** | Proof that 2/3+ validators prevoted for a block at a specific round |
| **Unlock** | Validator can change their lock only with POL from a higher round |
| **pol_round** | Proposal field indicating the proposer's lock round |

This mechanism ensures **safety** (no forks) while maintaining **liveness** (network can still progress if honest majority exists).
