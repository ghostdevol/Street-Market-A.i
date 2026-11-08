# StreetMarket AI — by GhostTech

**A street of storefronts, each run by an AI agent that works for its owner — bidding for jobs, undercutting rivals, and never, ever bidding below its own floor.**

Owners set standing orders once (the *jobsheet*: tasks, currencies, prices, costs, margin, bid step). The agents do the rest: watch the job board, quote, bid, win, and settle — agent to agent, in the open, on NEAR.

> **Live demo:** this repo's `index.html`, served by GitHub Pages — the whole street on one page: storefronts, jobsheets, the price chalkboard, the bid war, and settlement, all clickable.

## The street

Three NEAR smart contracts and a keeper:

- **Storefront Agent** (`contracts/storefront-agent`) — one contract per shop, same code every time. Holds the owner's jobsheet and quotes bids with checked arithmetic: `floor = cost + cost × margin`, undercut the best rival by one step, clamp *at* the floor, walk away rather than lose money. Simple rule: *kill, don't be killed.*
- **Price Bulletin** (`contracts/price-bulletin`) — the street's shared chalkboard. An authorized keeper posts live micro-USD prices (NEAR, BTC, ETH, USDC, USDT, wNEAR, ZEC); reads are freshness-gated — a stale price is flagged and refused, never silently traded on. One board, one truth, for every shop.
- **Street Board** (`contracts/street-board`) — the marketplace, in five modules: Job Registry, Auction Engine, Bid Validation (registered storefronts, task + currency authorization, price freshness), Award Engine (winner re-validated at close), and Settlement.
- **Keeper** (`keeper/`) — the chalk-holder: fetches real prices on an interval (provider chain with fallbacks, adapted from the author's own pre-existing pricing code) and posts them to the bulletin and the board. The script is written; its first live run against the deployed contracts is the next step.

The browser page is the *face*: it renders storefronts from a registry configuration — nothing hard-coded — so the same page fronts 3 shops or 3,000.

## Deployed on NEAR testnet — live now

Deployed Nov 8, 2026 from `ghosttech.testnet` and verified by reading the chain back:

| Contract | Account | State |
|---|---|---|
| Price Bulletin | [`bulletin.ghosttech.testnet`](https://testnet.nearblocks.io/address/bulletin.ghosttech.testnet) | Live — freshness window 60 s |
| Street Board | [`board.ghosttech.testnet`](https://testnet.nearblocks.io/address/board.ghosttech.testnet) | Live |
| Storefront — corner-cafe | [`cafe2.ghosttech.testnet`](https://testnet.nearblocks.io/address/cafe2.ghosttech.testnet) | Live — registered on the Board (`is_registered` = true) |
| Storefront — roaster-supply | [`roaster.ghosttech.testnet`](https://testnet.nearblocks.io/address/roaster.ghosttech.testnet) | Live — registered on the Board (`is_registered` = true) |
| Storefront — night-owl | [`owl.ghosttech.testnet`](https://testnet.nearblocks.io/address/owl.ghosttech.testnet) | Live — registered on the Board (`is_registered` = true) |

*Stated plainly: the first cafe account, `cafe.ghosttech.testnet`, is deployed but **not** registered — its signing key was generated in-memory by the first deploy run and lost when that run ended, so it can never sign again. The registered corner-cafe seat is `cafe2.ghosttech.testnet`, whose key is saved with the deploy tooling.*

## Settlement, stated honestly

- **NEAR jobs** escrow real NEAR in the Board at posting and release it on verified completion. Settled means *paid*.
- **Non-NEAR jobs** (USDC, BTC, ZEC…) create an **obligation** at award — a recorded debt, not secured funds. Its lifecycle is explicit: `Pending → Fulfilled` (settlement layer reports the asset moved, with a transaction reference) `→ Failed`, or `→ Expired`. The job is only ever called Settled when the obligation is Fulfilled.
- If a poster never confirms completion, the winner can claim after a timeout window (deemed acceptance). Escrowed money is never hostage to a silent poster.
- Freshness gates exactly two moments — accepting a bid, and awarding. A stale feed refuses an award and the job stays open; it never blocks the release of already-escrowed funds.

## Proof

**28 contract tests, all passing** (run `cargo test` in each contract folder):

- Storefront Agent — **10/10**: floor math, undercut + clamp, walk-aways, overflow traps, sub-floor configuration refused; plus a three-shop auction simulation in which the winner settles above its floor.
- Price Bulletin — **5/5**: keeper posts, fresh reads, conversions, staleness refusal, unauthorized posters rejected.
- Street Board — **13/13**: full post → bid → award → settle flow, escrow lock/release, obligation lifecycle, stale-at-award refusal and recovery, timeout claim.

## Judge steps — do X, see Y

1. **Open the demo page →** three storefronts with jobsheets, a live price chalkboard, an empty job board.
2. **Run the demo player →** a job posts, three agents bid down to their floors, the lowest eligible bid wins, settlement states tick over.
3. **Fast-forward the price feed past 60 s →** the board declares the feed stale and Bid Validation refuses the next bid. Re-sync and bidding resumes.
4. **Award a NEAR job and confirm completion →** escrow releases to the winner. Award a USDC job instead → it sits at *AwaitingSettlement*, paid-out still zero, until a fulfillment report with a reference arrives.
5. **Push the street-size selector →** the registry generates storefronts by the thousand from the same configuration.
6. **Check the chain itself →** open `board.ghosttech.testnet` on [testnet.nearblocks.io](https://testnet.nearblocks.io/address/board.ghosttech.testnet) and call `is_registered` for `cafe2.ghosttech.testnet`, `roaster.ghosttech.testnet`, and `owl.ghosttech.testnet` — all three answer **true**. Call `get_max_age_seconds` on `bulletin.ghosttech.testnet` — it answers **60**.

## Tracks

Autonomous Business · Agent Economies · AI Swarms · Real-World Money

## Scaling

One agent contract, stamped out per storefront; one board + bulletin pair per street; a page that renders whatever the registry holds. The factory/registry contract that stamps new storefronts on demand is the named next contract.

## Roadmap, plainly

- Contracts: built, test-proven (this repo), and **deployed on NEAR testnet** (Nov 8, 2026 — addresses above, all five verified on-chain). Remaining on this leg: the keeper's first live run, posting real prices to the deployed Bulletin on an interval.
- Cross-asset obligations settle through NEAR Intents — the fulfillment-report door in the Board is built for exactly that report.
- The page's in-browser bulletin remains a labeled simulation for the demo; the Bulletin contract it mirrors is the one live at `bulletin.ghosttech.testnet`. Wiring the page to read the chain is a later step — the keeper has not posted yet, so there is no live price history to read.

## License

MIT — see `LICENSE`.

Built by Daniel "GhostDev" Sielaff & Jinx (Muse) — GhostTech.
