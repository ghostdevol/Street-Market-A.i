# Storefront Agent — contract v1 build record (2026-11-08)

**What it is:** one NEAR smart contract = one storefront's agent (Daniel's spec:
jobsheet set by owner; undercut bidding with a hard floor; no LLM in the money
path; the AI face is a separate front-end layer that holds no keys).

**Source:** `src/lib.rs` (near-sdk 5.30, `#[near(contract_state)]` form).

**Jobsheet (owner-only writes):** tasks, listed currencies, per-item price +
cost, margin (bps), bid step. Owner-only enforced — a stranger's write panics.

**Trading rules (pure functions, host-tested):**
- floor = cost + margin (770 cost, 520 bps → floor 810)
- rival 850, step 10 → bid 840 (undercut by one step)
- rival 815 → bid clamps AT floor 810, never below
- rival at/below 810 → pass (walk away)
- no rival → open at asking if ≥ floor, else pass
- settle_win refuses any amount below the floor (panics), credits treasury at/above it

**Proof (2026-11-08 03:52–03:54 CST):**
- `cargo test`: **9 passed / 0 failed** (floor math, undercut, clamp, walk-away,
  opening bids, jobsheet respect, settle credit, settle refusal, owner lock)
- Wasm: `target/wasm32-unknown-unknown/release/storefront_agent.wasm`
  = **173,091 B, MD5 `1f34a549a3a1dc05f57b734e0de96220`**

**Build notes:** near-sdk 5.30 needs `borsh` as a direct dependency and the
`unit-testing` dev-dependency feature for host tests; wasm state macro is
`#[near(contract_state)]`.

**Honest scope line:** v1 treasury is the contract's internal ledger — the
test rig. Real-asset settlement (USDC/BTC/ZEC/NEAR) rides the platform's
NEAR Intents leg, the named milestone before submission.

**Next:** deploy to NEAR testnet as three shop accounts (needs a funded
testnet account), then the job-board platform + browser UI around it.

## Addendum — v2 (Daniel's revision) + simulation (2026-11-08 04:05–04:11)

Daniel handed over his own revision of the contract the same night; it
supersedes v1 (v1 kept as `v1-jinx-lib.rs.bak`). His changes: checked
(Option) floor math with overflow traps; prices/costs keyed by
(currency, item); set_item refuses a price below floor at configuration
time; spending caps as policy (the contract does NOT custody tokens —
settlement belongs to the marketplace layer); quote_bid quotes, the board
submits; job counters owner-attested and labeled as such.
His tests: **10 passed / 0 failed**. Wasm: **185,174 B, MD5
`9db9a94e3acaabd6e1cafdf540c19592`**.
Simulation (`tests/simulation.rs`, real quote_bid path, 3 shops, 1 job):
asking 900 → bids 900/880/875/865/850, night-owl passes at floor 864,
840/825/815/800, corner-cafe passes at floor 810, **roaster-supply wins
@ 800** (cost 730, floor 759, keeps 70 over cost). PASS.
Finding: quote_bid can quote above the shop's own list price when the
best bid is higher — the market driver caps at list price for now;
folding the cap into quote_bid is Daniel's call at board-build time.
