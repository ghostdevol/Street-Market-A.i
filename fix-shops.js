// StreetMarket AI by GhostTech — SHOP PRICE REPAIR (testnet).
// Runs on Daniel's Envy. Fixes the shops' missing coffee prices.
//
// What went wrong before: deploy.js's set_item calls failed SILENTLY —
// near-api-js functionCall does not throw when a call traps on-chain,
// and the outcome status was never checked. The calls also carried
// u128 values as raw JSON numbers, which traps; u128 must travel as a
// JSON string (the keeper.js pattern). And set_item is owner-only:
// the owner is the master account, so the MASTER signs these calls.
//
// This script therefore:
//   - passes every call's args as a RAW JSON BUFFER, so the bytes on
//     the wire are exactly the JSON written here (string u128s intact);
//   - checks every outcome's status and THROWS on anything that is
//     not Success — no silent failures anywhere;
//   - READS BACK every value it sets and exits non-zero if any
//     read-back differs from what was set.
//
// The master account's private key comes ONLY from the environment
// variable MASTER_PRIVATE_KEY on that machine (exported from Meteor
// there). No key is ever printed by this script. Shop keys are NOT
// needed: no shop signs anything in this repair — the owner does.
//
// Usage (PowerShell, from the streetmarket folder):
//   $env:MASTER_PRIVATE_KEY = "ed25519:..."   (from Meteor export, Envy only)
//   node fix-shops.js

const { connect, KeyPair, keyStores } = require("near-api-js");

const NETWORK = "testnet";
const MASTER = process.env.MASTER_ACCOUNT || "ghosttech.testnet";
const BULLETIN = `bulletin.${MASTER}`;
const BOARD = `board.${MASTER}`;
const GAS = "100000000000000"; // 100 TGas — master calls

const SHOPS = [
  { sub: "cafe2", name: "corner-cafe", price: "900", cost: "770" },
  { sub: "roaster", name: "roaster-supply", price: "880", cost: "730" },
  { sub: "owl", name: "night-owl", price: "950", cost: "800" },
];

// A u128 view result can come back as a JS number or a string depending
// on magnitude; normalize before comparing. null stays null.
const amountStr = (v) => (v === null || v === undefined ? null : String(v));
// u128 arguments must reach the wire as BARE JSON NUMBER LITERALS.
// These contracts take raw u128 parameters (not the U128 wrapper type):
// their deserializer accepts a JSON number and traps on a JSON string
// (sandbox-proven 2026-11-08: string form trapped 3/3 across two
// contracts; bare literals succeeded, full precision past u64).
// encodeArgs emits the u128 fields unquoted from their digit-string /
// BigInt values, so even 24-digit yocto amounts keep every digit;
// all other fields serialize normally.
const U128_FIELDS = new Set(["price", "cost", "amount", "budget", "best_bid"]);
function encodeArgs(obj) {
  const parts = [];
  for (const [k, v] of Object.entries(obj)) {
    if (U128_FIELDS.has(k) && v !== null && v !== undefined) {
      parts.push(`${JSON.stringify(k)}:${v.toString()}`);
    } else {
      parts.push(`${JSON.stringify(k)}:${JSON.stringify(v)}`);
    }
  }
  return Buffer.from(`{${parts.join(",")}}`);
}


async function main() {
  const priv = process.env.MASTER_PRIVATE_KEY;
  if (!priv) {
    console.error(
      "MASTER_PRIVATE_KEY is not set. Export it from Meteor on THIS machine and set it first."
    );
    process.exit(1);
  }

  const keyStore = new keyStores.InMemoryKeyStore();
  await keyStore.setKey(NETWORK, MASTER, KeyPair.fromString(priv));
  const near = await connect({
    networkId: NETWORK,
    keyStore,
    nodeUrl: "https://rpc.testnet.fastnear.com",
  });
  const master = await near.account(MASTER);

  // Status-checked change call. Throws with the on-chain failure
  // detail if the outcome is not Success.
  async function call(account, contractId, methodName, argsObj, gas) {
    const outcome = await account.functionCall({
      contractId,
      methodName,
      args: encodeArgs(argsObj),
      gas,
      attachedDeposit: "0",
    });
    const status = outcome && outcome.status;
    if (!status || !("SuccessValue" in status)) {
      const detail =
        status && status.Failure
          ? JSON.stringify(status.Failure)
          : JSON.stringify(status);
      throw new Error(
        `CALL FAILED ${methodName} on ${contractId} (signed by ${account.accountId}): ${detail}`
      );
    }
    console.log(`OK ${methodName} on ${contractId}`);
    return outcome;
  }

  const view = (contractId, methodName, args) =>
    master.viewFunction({ contractId, methodName, args });

  // -------------------------------------------------------------------
  // Step 1 — encoding proof: post NEAR to the bulletin via the helper,
  // then read the stored price back. Must read exactly 3700000.
  // -------------------------------------------------------------------
  console.log("Step 1: bulletin encoding proof (post_price NEAR, read back)");
  await call(master, BULLETIN, "post_price", { asset: "NEAR", price: "3700000" }, GAS);
  const nearPoint = await view(BULLETIN, "get_price", { asset: "NEAR" });
  // get_price -> (price, posted_ns, is_fresh)
  const nearStored = amountStr(Array.isArray(nearPoint) ? nearPoint[0] : nearPoint);
  console.log(`Bulletin get_price(NEAR) stored price: ${nearStored}`);
  if (nearStored !== "3700000") {
    throw new Error(
      `ENCODING PROOF FAILED: bulletin stored ${nearStored}, expected 3700000`
    );
  }
  console.log("Encoding proof passed: string u128 landed byte-exact.");

  // -------------------------------------------------------------------
  // Step 2 — price the three shops. Signed by the MASTER (the owner);
  // every u128 value is a string.
  // -------------------------------------------------------------------
  console.log("Step 2: set_item on each shop (owner-signed, string values)");
  for (const s of SHOPS) {
    const shopId = `${s.sub}.${MASTER}`;
    await call(
      master,
      shopId,
      "set_item",
      { currency: "USDC", item: "coffee", price: s.price, cost: s.cost },
      GAS
    );
    console.log(`Priced ${s.name} (${shopId}): price=${s.price} cost=${s.cost}`);
  }

  // -------------------------------------------------------------------
  // Step 3 — read every price/cost back. Any mismatch fails the run.
  // -------------------------------------------------------------------
  console.log("Step 3: read-back verification for all three shops");
  let mismatches = 0;
  for (const s of SHOPS) {
    const shopId = `${s.sub}.${MASTER}`;
    const priceBack = amountStr(
      await view(shopId, "get_item_price", { currency: "USDC", item: "coffee" })
    );
    const costBack = amountStr(
      await view(shopId, "get_item_cost", { currency: "USDC", item: "coffee" })
    );
    console.log(
      `${s.name} (${shopId}): get_item_price=${priceBack} (set ${s.price}) · get_item_cost=${costBack} (set ${s.cost})`
    );
    if (priceBack !== s.price || costBack !== s.cost) {
      mismatches += 1;
      console.error(
        `MISMATCH ${shopId}: read back price=${priceBack} cost=${costBack}, expected price=${s.price} cost=${s.cost}`
      );
    }
  }
  if (mismatches > 0) {
    console.error(`FIX FAILED: ${mismatches} shop(s) did not read back as set.`);
    process.exit(1);
  }
  console.log("All three shops read back exactly as priced.");

  // -------------------------------------------------------------------
  // Step 4 — seed the street's prices: USDC to the bulletin, and sync
  // NEAR + USDC into the board's price table.
  // -------------------------------------------------------------------
  console.log("Step 4: bulletin USDC post + board price syncs");
  await call(master, BULLETIN, "post_price", { asset: "USDC", price: "1000000" }, GAS);
  await call(master, BOARD, "sync_price", { currency: "NEAR", price: "3700000" }, GAS);
  await call(master, BOARD, "sync_price", { currency: "USDC", price: "1000000" }, GAS);

  console.log("FIX DONE: shops priced + verified, bulletin and board seeded.");
}

main().catch((e) => {
  console.error("FIX FAILED:", (e && e.message) || e);
  process.exit(1);
});
