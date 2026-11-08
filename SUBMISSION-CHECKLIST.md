# StreetMarket AI by GhostTech — Submission Readiness Checklist

**Deadline: submission closes Nov 14, 2026, 23:59 UTC (17:59 CST).** One submission per project; it cannot be edited after sending. A mistake is fixed through the organizers' request-changes page — never by submitting a second time.

## Contracts & tests

- [x] Storefront Agent contract built — Wasm 185,174 B, MD5 `9db9a94e3acaabd6e1cafdf540c19592`
- [x] Price Bulletin contract built — Wasm 152,192 B, MD5 `504bf0034294c574f1b8f12ed02b1023`
- [x] Street Board contract built (v2, settlement hardening) — Wasm 243,993 B, MD5 `1bdf6a5f24eb7f073a4265cb829ae289`
- [x] Tests green: Agent 10/10, Bulletin 5/5, Board 13/13 — **28/28**, plus the three-shop auction simulation
- [x] Build/test/deploy/demo instructions written so a judge can reproduce them (README, "Reproduce it")

## Deployment & on-chain proof

- [x] Price Bulletin deployed — `bulletin.ghosttech.testnet` (freshness window answers 60)
- [x] Street Board deployed — `board.ghosttech.testnet`
- [x] Three storefronts deployed and priced — `cafe2.ghosttech.testnet`, `roaster.ghosttech.testnet`, `owl.ghosttech.testnet`
- [x] All three shops registered on the board — `is_registered` answers true for each
- [x] Stated plainly in the README: the original `cafe.ghosttech.testnet` is deployed but **not** registered (its key was lost in the first deploy run); `cafe2` holds the registered corner-cafe seat
- [ ] Keeper's first live run (optional hardening) — script written (`keeper/keeper.js`), not yet run; no live prices posted to the bulletin yet

## Repo & docs

- [x] Repository public on GitHub, with a LICENSE file (MIT)
- [x] README carries the pitch, the deployed addresses, the honest settlement section, and judge steps (do X — see Y)
- [x] Honest labels intact: the in-page bulletin is a labeled in-browser simulation; the keeper has not posted live prices; non-NEAR settlement is an obligation until fulfilled
- [ ] This checklist's open items below all closed

## Demo & video

- [x] Hosted demo live on GitHub Pages (this repo's `index.html`): storefronts, jobsheet board, price bulletin simulation, settlement flow
- [x] Storyboard demo player fixed — Play advances a large stage frame by frame; tap-to-jump works
- [x] Demo video v1 built (75 s, narrated, captions burned in) from the page's story
- [ ] Real on-chain demo run: `node demo-run.js` — a USDC job bid down by the shops' own `quote_bid` calls through award → AwaitingSettlement → Fulfilled, and a NEAR job with real escrow locked and released on completion
- [ ] Demo transcript saved (`demo-transcript.txt` from the run) and linked or committed for judges
- [ ] Final demo video recorded from the **real on-chain run** (replaces or accompanies v1)

## X post (Daniel's own account, published by Daniel)

- [ ] Post published with the final demo video attached
- [ ] Post states the project was built at NEAR HACKATHON
- [ ] Post tags **@NEARProtocol** and **#NEARHackathon**
- [ ] Post URL copied for the submission form's `demoUrl` field

## Submission form (one shot — check every field before sending)

- [ ] `requestId` — a UUID, generated once and reused on any retry
- [ ] `projectName` — StreetMarket AI by GhostTech
- [ ] `leadName` — Daniel's name as team lead
- [ ] `email` — Daniel's contact email
- [ ] `size` — team size, exactly one of the form's listed values
- [ ] `tracks` — `business`, `swarms`, `agents`, `realworld`
- [ ] `nearAddress` — **`ghostech.near`** (mainnet payout account — spelled with ONE t; the testnet deploy account `ghosttech.testnet` is a different account and is NOT the payout address)
- [ ] `projectUrl` — the public GitHub repo URL
- [ ] `demoUrl` — the X post URL from the section above
- [ ] `idea` — 20–2000 characters: the problem, the product, and how NEAR fits
- [ ] `consent` — `true`, after agreeing to the contact notice
- [ ] **`website` field left EMPTY** — it is a spam trap; any non-empty value rejects the submission
- [ ] No extra fields added; payload reviewed in full by Daniel before sending
- [ ] Submitted once, before Nov 14, 2026, 23:59 UTC
