# Price Bulletin — contract build record (2026-11-08)

**What it is:** the street's shared chalkboard — one NEAR contract holding
live micro-USD prices (NEAR, USDC, USDT, wNEAR, BTC, ETH, ZEC) that every
storefront reads, so all agents price and bid against the same numbers.

**Mechanics (honest):** the chain cannot fetch web prices. An authorized
keeper posts prices; the board enforces freshness. Keeper pricing logic is
adapted from the author's own pre-existing pricing code:
CoinGecko primary with a DexScreener fallback, plus Uniswap V2/V3 pool
math as an API-independent source.

**Rules proven by tests (5/5 PASS, 2026-11-08 04:17 CST):**
- keeper posts, shops read fresh; 10 NEAR @ $3.70 converts to 37 USDC
- price older than max_age (60 s in tests) is flagged stale and the
  fresh-only read refuses it (conversion returns None)
- a stranger cannot post prices (panics)
- the keeper cannot replace itself — updater changes are owner-only
- convert math: overflow and zero target price return None

**Wasm:** `target/wasm32-unknown-unknown/release/price_bulletin.wasm`
= **152,192 B, MD5 `504bf0034294c574f1b8f12ed02b1023`**

**Next:** the keeper script itself (posting on
an interval) lands with the testnet deployment step.

**Correction (2026-11-08):** the keeper as shipped (`keeper/keeper.js`)
implements CoinGecko pricing plus the USDC/USDT $1.00 pegs only. The
DexScreener fallback and Uniswap V2/V3 pool math named above were source
material consulted during design; they are NOT implemented in the shipped
keeper. The README's keeper description matches the code.
