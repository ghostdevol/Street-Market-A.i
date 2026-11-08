# Street Board — contract build record (2026-11-08)

**What it is:** the marketplace contract — the street itself — built to
Daniel's five-module spec handed over 2026-11-08 04:19:
Job Registry / Auction Engine / Bid Validation / Award Engine / Settlement.

**Design points:**
- Reverse auction: shops bid DOWN; lowest eligible bid wins; bids must
  strictly improve and stay within the job budget.
- Bid Validation: registered storefronts only; the bidder's registered
  jobsheet must cover the job's task and currency; the job currency's
  keeper-synced price must be FRESH or the bid is refused.
- Award re-validates the winner (still registered, price still fresh);
  otherwise the job expires and escrow refunds.
- Settlement: NEAR jobs lock real escrow at posting (attached deposit ==
  budget) and release only on the poster's confirm_completion. Other
  currencies record the obligation at award; asset movement settles via
  the platform's Intents layer (named stage).

**Proof (2026-11-08 04:24 CST): `cargo test` — 9 passed / 0 failed:**
full street flow (post → 880/850 bids → award → settle, paid_out 850);
NEAR escrow lock (5 NEAR) and release (4 NEAR bid paid, escrow zeroed);
unregistered bidder rejected; wrong-task storefront rejected; stale
price blocks bidding; non-improving bid rejected; no-bid job expires and
refunds; poster cancels open job; stranger cannot confirm completion.

**Wasm:** `target/wasm32-unknown-unknown/release/street_board.wasm`
= **228,215 B, MD5 `a25bdcf27e31605fe8f5a3dc5b5c30be`**

**Street total, night one:** Agent 10/10 (+ simulation) · Bulletin 5/5 ·
Board 9/9 — 24 contract tests green, three Wasm contracts built.
Next: testnet deployment (funded testnet accounts), the keeper script,
then the face/browser UI, then the Intents settlement leg.

## v2 — settlement hardening (2026-11-08 04:26, Daniel's review)

Daniel's scrutiny, verbatim targets: (1) a non-NEAR obligation must never
be mistaken for secured funds — it needs pending/fulfilled/failed/expired
states; (2) test price going stale after the winning bid, before
settlement; (3) test a poster who never confirms completion.

Implemented:
- SettlementState {None, Pending, Fulfilled, Failed, Expired} on every
  job. confirm_completion on a non-NEAR job now lands at
  AwaitingSettlement with paid_out UNTOUCHED; only
  report_settlement_fulfilled(reference) may mark it Settled.
  report_settlement_failed and expire_settlement close the other doors.
- Freshness gates bids and awards ONLY. Stale at award -> close_auction
  REFUSES and the job stays Open (sync + retry recovers it); stale after
  award cannot block NEAR escrow release.
- Poster ghosting: claim_after_timeout — winner-only, after the
  completion window (deemed acceptance); NEAR escrow releases, non-NEAR
  advances to AwaitingSettlement still Pending.

Tests: **13 passed / 0 failed** (9 originals adapted + obligation
lifecycle, stale-at-award refusal/recovery, stale-after-award NEAR
settlement, timeout claim incl. too-early refusal).
Wasm: **243,993 B, MD5 `1bdf6a5f24eb7f073a4265cb829ae289`**.
