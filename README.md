# StreetMarket AI — by GhostTech

**A street of storefronts, each run by an AI agent that works for its owner — bidding for jobs, undercutting rivals, and never, ever bidding below its own floor.**

Owners set standing orders once (the *jobsheet*: tasks, currencies, prices, costs, margin, bid step). The agents do the rest: watch the job board, quote, bid, win, and settle — agent to agent, in the open, on NEAR.

> **Live demo:** this repo's `index.html`, served by GitHub Pages — the whole street on one page: storefronts, jobsheets, the price chalkboard, the bid war, and settlement, all clickable.

## The street

Three NEAR smart contracts and a keeper:

- **Storefront Agent** (`contracts/storefront-agent`) — one contract per shop, same code every time. Holds the owner's jobsheet and quotes bids with checked arithmetic: `floor = cost + cost × margin`, undercut the best rival by one step, clamp *at* the floor, walk away rather than lose money. Simple rule: *kill, don't be killed.*
- **Price Bulletin** (`contracts/price-bulletin`) — the street's shared chalkboard. An authorized keeper posts live micro-USD prices (NEAR, BTC, ETH, USDC, USDT, wNEAR, ZEC); reads are freshness-gated — a stale price is flagged and refused, never silently traded on. One board, one truth, for every shop.
- **Street Board** (`contracts/street-board`) — the marketplace, in five modules: Job Registry, Auction Engine, Bid Validation (registered storefronts, task + currency authorization, price freshness), Award Engine (winner re-validated at close), and Settlement.
- **Keeper** (`keeper/`) — the chalk-holder: on an interval it fetches real USD prices from CoinGecko (NEAR, BTC, ETH, ZEC; wNEAR tracks NEAR) and posts USDC/USDT at their $1.00 peg, writing every price to the bulletin (`post_price`) and the board (`sync_price`). The script is written; its first live run against the deployed contracts is the next step.

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

## Reproduce it

Everything below is run from the root of this repository unless a step says otherwise. Build and test commands are shown in shell syntax (macOS / Linux / WSL / Git Bash all work). Deploy, keeper, and demo steps were run by the operator on Windows, so their environment-variable lines are shown in PowerShell, with the bash equivalent noted once.

**Secrets rule, once:** private keys are supplied only through environment variables, from the operator's own wallet export, on the operator's machine. They are never written into this repo. The only key-bearing file the tooling creates is `shop-keys.json` (see step 4) — it lives on the operator's machine and must never be committed.

### 0. What is in the repo

- `contracts/storefront-agent`, `contracts/price-bulletin`, `contracts/street-board` — the three contracts (Rust, near-sdk 5, borsh 1).
- `wasm/` — the prebuilt Wasm for all three, byte-identical to the builds in each contract's `BUILD-RECORD.md`:

| File | Size | MD5 |
|---|---|---|
| `wasm/price_bulletin.wasm` | 152,192 B | `504bf0034294c574f1b8f12ed02b1023` |
| `wasm/storefront_agent.wasm` | 185,174 B | `9db9a94e3acaabd6e1cafdf540c19592` |
| `wasm/street_board.wasm` | 243,993 B | `1bdf6a5f24eb7f073a4265cb829ae289` |

- `deploy.js` — the testnet deployment script (phases in step 4).
- `keeper/keeper.js` — the price keeper (step 5).
- `demo-run.js` — the on-chain demo (step 6).
- `index.html` — the browser demo, also served live by GitHub Pages at https://ghostdevol.github.io/Street-Market-A.i/

### 1. Prerequisites

Rust with the NEAR Wasm target:

```
rustup target add wasm32-unknown-unknown
rustc --version
cargo --version
```

Node.js 18 or newer (the keeper uses the built-in `fetch`) with npm:

```
node --version
npm --version
```

The deploy, keeper, and demo scripts use `near-api-js` **^2.1.4** (installed in steps 4–6). Deployment and the demo also need a funded NEAR **testnet** account whose key you can export from your wallet — the existing deployment used `ghosttech.testnet`.

### 2. Build the contracts

Storefront Agent:

```
cd contracts/storefront-agent
cargo build --target wasm32-unknown-unknown --release
cd ../..
```

Price Bulletin:

```
cd contracts/price-bulletin
cargo build --target wasm32-unknown-unknown --release
cd ../..
```

Street Board:

```
cd contracts/street-board
cargo build --target wasm32-unknown-unknown --release
cd ../..
```

Each build leaves its Wasm at `contracts/<crate>/target/wasm32-unknown-unknown/release/<name>.wasm` with the file name from the table in step 0. Verify a fresh build against the shipped copies:

```
md5sum wasm/price_bulletin.wasm wasm/storefront_agent.wasm wasm/street_board.wasm
```

The three MD5s must match the table in step 0.

### 3. Run the tests

Storefront Agent — expect **10 passed, 0 failed**, plus the three-shop auction simulation in `tests/simulation.rs` passing as its own test (asking 900 → roaster-supply wins at 800, above its floor of 759):

```
cd contracts/storefront-agent
cargo test
cd ../..
```

Price Bulletin — expect **5 passed, 0 failed**:

```
cd contracts/price-bulletin
cargo test
cd ../..
```

Street Board — expect **13 passed, 0 failed**:

```
cd contracts/street-board
cargo test
cd ../..
```

Total: **28 contract tests passing** (10 + 5 + 13), matching each contract's `BUILD-RECORD.md`.

### 4. Deploy to NEAR testnet (how the live deployment was made)

Install the script dependency at the repo root:

```
npm install near-api-js@^2.1.4
```

`deploy.js` reads the three Wasm files from the folder it runs in, so stage them next to it:

```
cp wasm/price_bulletin.wasm wasm/street_board.wasm wasm/storefront_agent.wasm .
```

Set the master account's key in the environment. The account ID is public; the key value comes from your own wallet export and is never typed into any file:

```
$env:MASTER_ACCOUNT = "ghosttech.testnet"
$env:MASTER_PRIVATE_KEY = "ed25519:..."
```

(In bash: `export MASTER_ACCOUNT=ghosttech.testnet` and `export MASTER_PRIVATE_KEY=` followed by the exported key. `MASTER_ACCOUNT` defaults to `ghosttech.testnet` if unset.)

**Phase 1 — core.** Creates `bulletin.<master>` (funded 1.7 NEAR) and `board.<master>` (funded 2.6 NEAR), deploys each Wasm, and initializes them from the master (bulletin: `max_age_seconds` 60; board: completion and settle windows 3600 s):

```
node deploy.js core
```

**Phase 2 — shops, as actually run.** The first attempt (`node deploy.js shops`, shops funded 2.0 NEAR) deployed and priced the original `cafe.ghosttech.testnet` but failed at its board registration: a shop funded 2.0 cannot prepay the 300 TGas the register call attached (it needs ~0.30 NEAR free; the shop had ~0.14 above its storage lock), and that run's shop keys were generated in memory and lost. The repair phase is what produced the live state — cafe re-dealt as `cafe2`, every shop funded 2.4 NEAR, each shop's keypair **saved to `shop-keys.json` before its account is created**, and registration at 50 TGas:

```
node deploy.js shops2
```

Expected last line: `SHOPS2 DONE: cafe2, roaster, owl deployed, priced, and registered on the board.`

Notes that matter for reproduction:

- `core` and `shops` create fresh subaccounts; re-running `core` against `ghosttech.testnet` fails because `bulletin.ghosttech.testnet` and `board.ghosttech.testnet` already exist. To redeploy from scratch, point `MASTER_ACCOUNT` at a different funded testnet master.
- `shops2` checks each shop first (exists? has code?) and is safe to re-run.
- `shop-keys.json` holds the three shops' private keys. The demo in step 6 needs it. Never commit it.

### 5. Run the keeper (written; not yet run against the live contracts)

```
cd keeper
npm install
```

Set the environment (account IDs are the live deployment's; the key is the master's, from your wallet export — on this deployment the bulletin updater and the board keeper are both `ghosttech.testnet`, so the keeper signs as the master):

```
$env:KEEPER_ACCOUNT_ID = "ghosttech.testnet"
$env:KEEPER_PRIVATE_KEY = "ed25519:..."
$env:BULLETIN_ACCOUNT = "bulletin.ghosttech.testnet"
$env:BOARD_ACCOUNT = "board.ghosttech.testnet"
$env:KEEPER_INTERVAL_MS = "30000"
```

(`KEEPER_INTERVAL_MS` is optional, default 30000; `NEAR_RPC_URL` and `NETWORK_ID` are also optional, defaulting to `https://rpc.testnet.near.org` and `testnet`.)

Start it:

```
node keeper.js
```

What it does, exactly as written: fetches USD prices from CoinGecko's simple/price endpoint for NEAR, BTC, ETH, and ZEC (wNEAR reuses NEAR's price), rounds each to micro-USD (1_000_000 = $1.00), sets USDC and USDT to the 1_000_000 peg, then calls `post_price` on the bulletin and `sync_price` on the board for every asset, and repeats on the interval. The keeper itself has not run yet. Prices WERE posted to the deployed bulletin during the Nov 8 demo (NEAR at 3700000 and USDC at 1000000 micro-USD, by the demo scripts); with the bulletin's 60-second freshness window they read as stale between posts until a keeper keeps them current — the freshness rule working as designed.

### 6. Run the on-chain demo

Prerequisites: step 4's root install is done, `MASTER_PRIVATE_KEY` is set as in step 4 (`MASTER_ACCOUNT` unset or `ghosttech.testnet`), and `shop-keys.json` from the `shops2` run is present in the repo root — the script signs bids as the shops themselves, and the shops' keys live only in that file.

```
node demo-run.js
```

What it does, against the live testnet contracts:

1. **USDC job** — posts a `coffee-run` / `coffee` job in USDC on the board. Each shop prices its own bid by calling its own `quote_bid`; bids go on the board in turn and strictly improve until the remaining shops pass at their floors. `close_auction` awards the lowest eligible bid; the poster confirms completion and the job lands at **AwaitingSettlement** with paid-out still zero; a settlement-fulfilled report then marks it **Settled**.
2. **NEAR job** — posts the same job in NEAR with the full budget attached, so real testnet NEAR locks in the board's escrow at posting. The shops bid the same way; on award and completion confirmation, the escrow **releases to the winner** — settled means paid.
3. Every step — job IDs, each bid, the award, the settlement states, escrow before/after — is written to **`demo-transcript.txt`** in the repo root, with the resulting on-chain state read back from the board.

**Result of the Nov 8, 2026 run** (committed as `demo-transcript.txt` in this repo, and independently re-read from the chain after the run):

- **Job 3 (USDC):** the three shops bid it down from 900 to **805** through their own `quote_bid` calls; roaster-supply won. Award left it at AwaitingSettlement with paid-out still 0; the settlement-fulfilled report (`streetmarket-demo-usdc-001`) then marked it **Settled / Fulfilled**, paid-out 805.
- **Job 4 (NEAR):** 0.5 NEAR locked in escrow at posting; bids 0.49 → 0.48 → 0.47 → 0.465 → **0.455**; roaster-supply won. The main run stopped on a display-line error after the auction (the transcript keeps that failure line — the record is unedited), and `finish-job-b.js` completed the award and confirmation: the escrow released **exactly 0.455 NEAR** to the winner (its balance delta, to the yocto) and the job's escrow reads back **0**.

### 7. Verify the on-chain state yourself

No trust in this README required. On [testnet.nearblocks.io](https://testnet.nearblocks.io), open each account from the table in "Deployed on NEAR testnet" — or call the contracts' view methods directly over JSON-RPC. One full example (answers `true`, returned as the byte array `[116, 114, 117, 101]`):

```
curl -s https://rpc.testnet.fastnear.com -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","id":"verify","method":"query","params":{"request_type":"call_function","account_id":"board.ghosttech.testnet","method_name":"is_registered","args_base64":"eyJhY2NvdW50IjoiY2FmZTIuZ2hvc3R0ZWNoLnRlc3RuZXQifQ==","finality":"final"}}'
```

Same call, other `args_base64` values (the JSON is shown so you can re-encode it yourself):

| Check | Contract | Method | Args (JSON) | `args_base64` | Expected |
|---|---|---|---|---|---|
| roaster registered | `board.ghosttech.testnet` | `is_registered` | `{"account":"roaster.ghosttech.testnet"}` | `eyJhY2NvdW50Ijoicm9hc3Rlci5naG9zdHRlY2gudGVzdG5ldCJ9` | `true` |
| owl registered | `board.ghosttech.testnet` | `is_registered` | `{"account":"owl.ghosttech.testnet"}` | `eyJhY2NvdW50Ijoib3dsLmdob3N0dGVjaC50ZXN0bmV0In0=` | `true` |
| bulletin freshness window | `bulletin.ghosttech.testnet` | `get_max_age_seconds` | `{}` | `e30=` | `60` |
| job count | `board.ghosttech.testnet` | `job_count` | `{}` | `e30=` | number of jobs posted (grows after step 6) |
| one job | `board.ghosttech.testnet` | `get_job` | `{"job_id":1}` | `eyJqb2JfaWQiOjF9` | `(status, currency, budget, best_bid, best_bidder, settlement_state)` |

After the demo run in step 6, `get_job` on its two job IDs shows the USDC job `Settled` with settlement `Fulfilled`, and the NEAR job `Settled` with its escrow released.

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
