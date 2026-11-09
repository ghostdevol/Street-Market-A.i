// StreetMarket AI by GhostTech — REAL testnet demo run.
// Runs on Daniel's Envy. Drives the deployed contracts through a full
// street flow, on-chain: prices posted and synced, a USDC bid war fought
// by the three storefronts' OWN quote_bid logic, an award, an obligation
// carried honestly to fulfillment, and a NEAR job whose escrow actually
// releases to the winner (balance deltas printed as proof).
//
// The master account's private key comes ONLY from the environment
// variable MASTER_PRIVATE_KEY on that machine (exported from Meteor
// there). Shop keys come from shop-keys.json in this folder (written by
// deploy.js shops2). No key is ever printed by this script.
//
// Usage (PowerShell, from the streetmarket folder, same window where
// deploy.js ran):
//   $env:MASTER_PRIVATE_KEY = "ed25519:..."   (from Meteor export, Envy only)
//   node demo-run.js
//
// Writes the full transcript to demo-transcript.txt in this folder.

const fs = require("fs");
const { connect, KeyPair, keyStores, utils } = require("near-api-js");

const NETWORK = "testnet";
const MASTER = process.env.MASTER_ACCOUNT || "ghosttech.testnet";
const BULLETIN = `bulletin.${MASTER}`;
const BOARD = `board.${MASTER}`;
const GAS_SHOP = "30000000000000"; // 30 TGas — shop calls (bid)
const GAS_MASTER = "100000000000000"; // 100 TGas — master calls

// Prices in micro-USD (the bulletin/board unit): 1_000_000 = $1.00.
const PRICE_NEAR = "3700000"; // NEAR at $3.70 — the demo page's own price
const PRICE_USDC = "1000000"; // USDC at the $1.00 peg

const SHOPS = [
  { sub: "cafe2", name: "corner-cafe" },
  { sub: "roaster", name: "roaster-supply" },
  { sub: "owl", name: "night-owl" },
];

const fmt = (yocto) => utils.format.formatNearAmount(String(yocto));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---------------------------------------------------------------------
// Transcript: every line goes to the console AND the transcript file.
// ---------------------------------------------------------------------
const lines = [];
function log(msg) {
  console.log(msg);
  lines.push(msg);
}
function writeTranscript() {
  fs.writeFileSync("demo-transcript.txt", lines.join("\n") + "\n");
}

// near-api-js returns a change/view call's return value decoded already,
// but a function-call outcome carries it base64-encoded in SuccessValue.
function callReturn(outcome) {
  const sv = outcome && outcome.status && outcome.status.SuccessValue;
  if (!sv) return null;
  return JSON.parse(Buffer.from(sv, "base64").toString());
}

// Status-checked change call: anything that is not Success throws
// with the on-chain failure detail, so a trapped call aborts the run
// through the DEMO FAILED path instead of passing silently
// (the deploy.js failure mode).
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

async function callChecked(account, contractId, methodName, argsObj, gas, attachedDeposit) {
  const outcome = await account.functionCall({
    contractId,
    methodName,
    args: encodeArgs(argsObj),
    gas,
    attachedDeposit,
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
  return outcome;
}

async function main() {
  const priv = process.env.MASTER_PRIVATE_KEY;
  if (!priv) {
    console.error("MASTER_PRIVATE_KEY is not set. Export it from Meteor on THIS machine and set it first.");
    process.exit(1);
  }
  if (!fs.existsSync("shop-keys.json")) {
    console.error("shop-keys.json not found in this folder — run `node deploy.js shops2` first (it writes the shop keys).");
    process.exit(1);
  }
  const savedKeys = JSON.parse(fs.readFileSync("shop-keys.json", "utf8"));

  const keyStore = new keyStores.InMemoryKeyStore();
  await keyStore.setKey(NETWORK, MASTER, KeyPair.fromString(priv));
  const near = await connect({
    networkId: NETWORK,
    keyStore,
    nodeUrl: "https://rpc.testnet.fastnear.com",
  });
  const master = await near.account(MASTER);

  const shopAcc = {};
  for (const s of SHOPS) {
    const id = `${s.sub}.${MASTER}`;
    if (!savedKeys[id]) {
      throw new Error(`No saved key for ${id} in shop-keys.json — re-run deploy.js shops2.`);
    }
    await keyStore.setKey(NETWORK, id, KeyPair.fromString(savedKeys[id]));
    shopAcc[id] = await near.account(id);
  }

  const balanceOf = async (acc) => (await acc.state()).amount;

  async function printJob(jobId, label) {
    // get_job -> (status, currency, budget, best_bid, best_bidder, settlement)
    const j = await viewRetry(BOARD, "get_job", { job_id: jobId });
    log(
      `${label} job=${jobId} status=${j[0]} currency=${j[1]} budget=${j[2]} ` +
        `best_bid=${j[3] === null ? "none" : j[3]} best_bidder=${j[4] === null ? "none" : j[4]} settlement=${j[5]}`
    );
    return j;
  }
  // View calls get retry armor: one flaky RPC view (a transient
  // WasmTrap from a lagging public node killed the 2026-11-08 22:24
  // run inside the auction loop, after Job A posted cleanly) must not
  // kill the run. Six tries, three seconds apart; only a persistent
  // failure aborts, through the same DEMO FAILED path.
  async function viewRetry(contractId, methodName, args) {
    let lastErr;
    for (let i = 1; i <= 6; i++) {
      try {
        return await master.viewFunction({ contractId, methodName, args });
      } catch (e) {
        lastErr = e;
        log(`VIEW RETRY ${i}/6 ${methodName} on ${contractId} - transient query failure, waiting 3s`);
        await new Promise((r) => setTimeout(r, 3000));
      }
    }
    throw lastErr;
  }


  // Price freshness on the board lasts 60 seconds. lastSync stamps the
  // last time each asset was posted to the bulletin AND synced to the
  // board (the keeper's own two-write pattern).
  const lastSync = { NEAR: 0, USDC: 0 };
  async function syncPrice(asset, price, force) {
    if (!force && Date.now() - lastSync[asset] < 30000) return;
    await callChecked(master, BULLETIN, "post_price", { asset, price }, GAS_MASTER, "0");
    await callChecked(master, BOARD, "sync_price", { currency: asset, price }, GAS_MASTER, "0");
    lastSync[asset] = Date.now();
    log(`PRICE ${asset} = ${price} micro-USD posted to bulletin and synced to board`);
  }

  log("==============================================================");
  log("StreetMarket AI by GhostTech — REAL NEAR testnet demo run");
  log(`Started ${new Date().toISOString()}`);
  log(`Poster/keeper: ${MASTER}`);
  log(`Bulletin: ${BULLETIN} · Board: ${BOARD}`);
  log("==============================================================");

  // -------------------------------------------------------------------
  // Step 1 — balances up front
  // -------------------------------------------------------------------
  log(`Master ${MASTER} balance: ${fmt(await balanceOf(master))} NEAR`);
  for (const s of SHOPS) {
    const id = `${s.sub}.${MASTER}`;
    log(`Shop ${s.name} (${id}) balance: ${fmt(await balanceOf(shopAcc[id]))} NEAR`);
  }

  // -------------------------------------------------------------------
  // Step 2 — keeper posts prices (bulletin) and syncs them (board)
  // -------------------------------------------------------------------
  await syncPrice("NEAR", PRICE_NEAR, true);
  await syncPrice("USDC", PRICE_USDC, true);
  await sleep(2000);

  // -------------------------------------------------------------------
  // Step 3 — JOB A: the USDC bid war, driven by each shop's quote_bid
  // -------------------------------------------------------------------
  const JOB_A_BUDGET = "900";
  const outA = await callChecked(
    master, BOARD, "post_job",
    { task: "coffee-run", item: "coffee", currency: "USDC", budget: JOB_A_BUDGET, duration_seconds: 3600 },
    GAS_MASTER, "0"
  );
  const jobA = callReturn(outA);
  log(`JOB_POSTED A id=${jobA} task=coffee-run item=coffee currency=USDC budget=${JOB_A_BUDGET} (no NEAR attached — a USDC job is an obligation, not escrow)`);
  await printJob(jobA, "STATE");

  let round = 0;
  for (;;) {
    round += 1;
    if (round > 50) throw new Error("Auction did not converge after 50 rounds");
    let bidsThisRound = 0;
    for (const s of SHOPS) {
      const id = `${s.sub}.${MASTER}`;
      const j = await viewRetry(BOARD, "get_job", { job_id: jobA });
      const best = j[3] === null ? null : BigInt(j[3]);
      const bestBidder = j[4];
      if (bestBidder === id) {
        log(`HOLD ${s.name} (${id}) — currently winning at ${best} USDC, never bids against itself`);
        continue;
      }
      // Freshness guard: keep the board's USDC price inside its window.
      await syncPrice("USDC", PRICE_USDC, false);
      // The shop's OWN contract decides its quote. best_bid = 0 is the
      // contract's opening sentinel (no rival bid yet -> open at asking).
      const q = await viewRetry(id, "quote_bid", encodeArgs({ task: "coffee-run", currency: "USDC", item: "coffee", best_bid: best === null ? "0" : best.toString() }));
      if (q === null) {
        log(`PASS ${s.name} (${id}) — no quote: the best bid is at or below its floor`);
        continue;
      }
      let quote = BigInt(q);
      if (quote > BigInt(JOB_A_BUDGET)) quote = BigInt(JOB_A_BUDGET); // never quote over budget
      if (best !== null && quote >= best) {
        log(`PASS ${s.name} (${id}) — quote ${quote} cannot beat best ${best} (floor reached)`);
        continue;
      }
      await callChecked(
        shopAcc[id], BOARD, "bid",
        { job_id: jobA, amount: quote.toString() },
        GAS_SHOP, "0"
      );
      log(`BID ${s.name} (${id}) ${quote} USDC on job ${jobA}`);
      bidsThisRound += 1;
    }
    if (bidsThisRound === 0) {
      log(`Auction over after round ${round}: a full round with no new bid.`);
      break;
    }
  }
  const finalA = await printJob(jobA, "AUCTION FINAL");
  const winnerA = finalA[4];
  const winningA = finalA[3];
  log(`WINNER A: ${winnerA} at ${winningA} USDC`);

  // -------------------------------------------------------------------
  // Step 4 — award Job A
  // -------------------------------------------------------------------
  await syncPrice("USDC", PRICE_USDC, true); // freshness is re-checked at award
  await callChecked(master, BOARD, "close_auction", { job_id: jobA }, GAS_MASTER, "0");
  log(`AWARD A: close_auction called by poster (expected: status=Awarded, settlement=Pending)`);
  await printJob(jobA, "STATE");

  // -------------------------------------------------------------------
  // Step 5 — poster confirms completion of Job A.
  // For a non-NEAR job this records verified work ONLY: the job moves
  // to AwaitingSettlement with the obligation still Pending. No funds
  // moved — proven below by job_escrow (the recorded debt) versus
  // paid_out_to (actual released payments: still zero).
  // -------------------------------------------------------------------
  await callChecked(master, BOARD, "confirm_completion", { job_id: jobA }, GAS_MASTER, "0");
  log(`CONFIRM A: poster confirmed completion (expected: status=AwaitingSettlement, settlement=Pending)`);
  const stateA = await printJob(jobA, "STATE");
  const escrowA = await viewRetry(BOARD, "job_escrow", { job_id: jobA });
  const paidA = await viewRetry(BOARD, "paid_out_to", { account: winnerA });
  log(
    `OBLIGATION, NOT FUNDS: job ${jobA} records ${escrowA} USDC owed to ${winnerA} (settlement=${stateA[5]}), ` +
      `while paid_out_to(${winnerA}) = ${paidA} — nothing has actually been paid yet.`
  );

  // -------------------------------------------------------------------
  // Step 6 — settlement layer reports the USDC actually moved
  // -------------------------------------------------------------------
  await callChecked(
    master, BOARD, "report_settlement_fulfilled",
    { job_id: jobA, reference: "streetmarket-demo-usdc-001" },
    GAS_MASTER, "0"
  );
  log(`SETTLE A: report_settlement_fulfilled ref=streetmarket-demo-usdc-001 (expected: status=Settled, settlement=Fulfilled)`);
  await printJob(jobA, "STATE");
  const paidA2 = await viewRetry(BOARD, "paid_out_to", { account: winnerA });
  const refA = await viewRetry(BOARD, "settlement_ref", { job_id: jobA });
  log(`PROOF A: paid_out_to(${winnerA}) = ${paidA2} USDC, settlement_ref = ${refA}`);

  // -------------------------------------------------------------------
  // Step 7 — JOB B: a NEAR job with REAL escrow (0.5 NEAR locked at
  // posting, released to the winner on confirmation).
  // -------------------------------------------------------------------
  const roasterId = `roaster.${MASTER}`;
  const roasterBefore = BigInt(await balanceOf(shopAcc[roasterId]));
  const masterBefore = BigInt(await balanceOf(master));
  log(`JOB B setup: roaster balance before = ${fmt(roasterBefore)} NEAR · master balance before = ${fmt(masterBefore)} NEAR`);

  const JOB_B_BUDGET = "500000000000000000000000"; // 0.5 NEAR in yocto
  const outB = await callChecked(
    master, BOARD, "post_job",
    { task: "coffee-run", item: "coffee", currency: "NEAR", budget: JOB_B_BUDGET, duration_seconds: 3600 },
    GAS_MASTER, JOB_B_BUDGET // NEAR jobs lock the full budget as escrow
  );
  const jobB = callReturn(outB);
  log(`JOB_POSTED B id=${jobB} task=coffee-run item=coffee currency=NEAR budget=${JOB_B_BUDGET} yocto (0.5 NEAR locked in escrow at posting)`);
  await printJob(jobB, "STATE");

  await syncPrice("NEAR", PRICE_NEAR, true); // bids refuse on a stale price
  const scriptedBids = [
    { sub: "cafe2", name: "corner-cafe", amount: "490000000000000000000000" },
    { sub: "owl", name: "night-owl", amount: "480000000000000000000000" },
    { sub: "roaster", name: "roaster-supply", amount: "470000000000000000000000" },
    { sub: "cafe2", name: "corner-cafe", amount: "465000000000000000000000" },
    { sub: "roaster", name: "roaster-supply", amount: "455000000000000000000000" },
  ];
  for (const b of scriptedBids) {
    const id = `${b.sub}.${MASTER}`;
    await syncPrice("NEAR", PRICE_NEAR, false); // freshness guard, >30 s only
    await callChecked(
      shopAcc[id], BOARD, "bid",
      { job_id: jobB, amount: b.amount },
      GAS_SHOP, "0"
    );
    log(`BID ${b.name} (${id}) ${b.amount} yocto (${fmt(b.amount)} NEAR) on job ${jobB}`);
  }
  const finalB = await printJob(jobB, "AUCTION FINAL");
  log(`WINNER B: ${finalB[4]} at ${finalB[3]} yocto (${fmt(finalB[3])} NEAR)`);

  await syncPrice("NEAR", PRICE_NEAR, true); // freshness is re-checked at award
  await callChecked(master, BOARD, "close_auction", { job_id: jobB }, GAS_MASTER, "0");
  log(`AWARD B: close_auction called by poster (expected: status=Awarded)`);
  await printJob(jobB, "STATE");

  await callChecked(master, BOARD, "confirm_completion", { job_id: jobB }, GAS_MASTER, "0");
  log(`CONFIRM B: poster confirmed completion — escrow releases to the winner (expected: status=Settled)`);
  await printJob(jobB, "STATE");

  const roasterAfter = BigInt(await balanceOf(shopAcc[roasterId]));
  const masterAfter = BigInt(await balanceOf(master));
  const dRoaster = roasterAfter - roasterBefore;
  const dMaster = masterAfter - masterBefore;
  log(`JOB B balances after: roaster = ${fmt(roasterAfter)} NEAR · master = ${fmt(masterAfter)} NEAR`);
  log(
    `BALANCE DELTAS B: roaster ${dRoaster < 0n ? "-" : "+"}${fmt(dRoaster < 0n ? -dRoaster : dRoaster)} NEAR ` +
      `(the 0.455 NEAR escrow release, minus the gas roaster spent placing its own bids)`
  );
  log(`BALANCE DELTAS B: master ${dMaster < 0n ? "-" : "+"}${fmt(dMaster < 0n ? -dMaster : dMaster)} NEAR (the 0.5 NEAR escrow it locked, plus its call gas)`);

  log("==============================================================");
  log(`DEMO DONE: job A (USDC) settled via obligation+fulfillment, job B (NEAR) settled via real escrow release.`);
  log(`Board explorer: https://testnet.nearblocks.io/address/${BOARD}`);
  log(`Finished ${new Date().toISOString()}`);
  log("==============================================================");
}

main()
  .then(() => {
    writeTranscript();
    console.log("Transcript written to demo-transcript.txt");
  })
  .catch((e) => {
    log(`DEMO FAILED: ${(e && e.message) || e}`);
    log("Transcript above holds everything that completed before the failure.");
    writeTranscript();
    console.error("Transcript written to demo-transcript.txt");
    process.exit(1);
  });
