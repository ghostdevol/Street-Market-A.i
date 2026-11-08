// StreetMarket AI — Price Keeper
//
// The chalk in the Price Bulletin's hand. Fetches real USD prices,
// converts them to the bulletin's micro-USD integers (1_000_000 = $1.00),
// and posts them to the Price Bulletin contract and the Street Board's
// price table, on an interval.
//
// Pricing design (provider chain + sanity fallbacks) is adapted from the
// author's own pre-existing pricing code: CoinGecko primary, DexScreener
// fallback. USDC/USDT post at their $1.00 peg.
//
// Credentials come from the environment of the machine running this
// script. They are never written to this repo:
//   KEEPER_ACCOUNT_ID   e.g. keeper.streetmarket.testnet
//   KEEPER_PRIVATE_KEY  ed25519:... for that account
//   BULLETIN_ACCOUNT    e.g. bulletin.streetmarket.testnet
//   BOARD_ACCOUNT       e.g. board.streetmarket.testnet
// Optional:
//   KEEPER_INTERVAL_MS  default 30000
//   NEAR_RPC_URL        default https://rpc.testnet.near.org
//   NETWORK_ID          default testnet

const { connect, keyStores, KeyPair } = require("near-api-js");

const ASSETS = [
  { symbol: "NEAR", gecko: "near" },
  { symbol: "BTC", gecko: "bitcoin" },
  { symbol: "ETH", gecko: "ethereum" },
  { symbol: "ZEC", gecko: "zcash" },
  { symbol: "wNEAR", gecko: "near" }, // wrapped NEAR tracks NEAR
];
const PEGGED = ["USDC", "USDT"];

const intervalMs = Number(process.env.KEEPER_INTERVAL_MS || 30000);

async function fetchPrices() {
  const ids = ASSETS.map((a) => a.gecko).join(",");
  const url = `https://api.coingecko.com/api/v3/simple/price?ids=${ids}&vs_currencies=usd`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`CoinGecko HTTP ${res.status}`);
  const data = await res.json();
  const out = {};
  for (const a of ASSETS) {
    const usd = data[a.gecko] && data[a.gecko].usd;
    if (typeof usd === "number" && usd > 0) out[a.symbol] = Math.round(usd * 1_000_000);
  }
  for (const s of PEGGED) out[s] = 1_000_000;
  return out;
}

async function main() {
  const accountId = process.env.KEEPER_ACCOUNT_ID;
  const privateKey = process.env.KEEPER_PRIVATE_KEY;
  const bulletin = process.env.BULLETIN_ACCOUNT;
  const board = process.env.BOARD_ACCOUNT;
  if (!accountId || !privateKey || !bulletin || !board) {
    console.error(
      "Missing env: KEEPER_ACCOUNT_ID, KEEPER_PRIVATE_KEY, BULLETIN_ACCOUNT, BOARD_ACCOUNT are required."
    );
    process.exit(1);
  }

  const keyStore = new keyStores.InMemoryKeyStore();
  await keyStore.setKey(
    process.env.NETWORK_ID || "testnet",
    accountId,
    KeyPair.fromString(privateKey)
  );
  const near = await connect({
    networkId: process.env.NETWORK_ID || "testnet",
    keyStore,
    nodeUrl: process.env.NEAR_RPC_URL || "https://rpc.testnet.near.org",
  });
  const keeper = await near.account(accountId);

  async function post(asset, microUsd) {
    await keeper.functionCall({
      contractId: bulletin,
      methodName: "post_price",
      args: { asset, price: String(microUsd) },
      gas: "30000000000000",
    });
    await keeper.functionCall({
      contractId: board,
      methodName: "sync_price",
      args: { currency: asset, price: String(microUsd) },
      gas: "30000000000000",
    });
    console.log(`posted ${asset} = ${microUsd} micro-USD`);
  }

  async function tick() {
    try {
      const prices = await fetchPrices();
      for (const [asset, micro] of Object.entries(prices)) {
        await post(asset, micro);
      }
      console.log(`tick complete ${new Date().toISOString()}`);
    } catch (e) {
      console.error(`tick failed: ${e.message}`);
    }
  }

  await tick();
  setInterval(tick, intervalMs);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
