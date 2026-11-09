// finish-job-b.js — StreetMarket AI by GhostTech
//
// Completion pass for the 2026-11-08 22:44 demo run. That run settled
// Job A (id 3, USDC) completely and ran Job B's whole bid war (id 4,
// NEAR): roaster won at 0.455 NEAR with 0.5 NEAR locked in escrow.
// The run then died on a DISPLAY line — formatting the view-returned
// best_bid (a lossy JS float, 4.55e+23) threw "Invalid character"
// BEFORE close_auction was ever sent. Job 4 was verified Open
// on-chain after the failure. This finisher does only the remaining
// steps: fresh price sync, close_auction, confirm_completion (the
// escrow release), and the balance proof. It APPENDS to
// demo-transcript.txt so the run keeps one continuous record.
//
// MASTER_PRIVATE_KEY comes from the environment only. No shop keys
// are needed: the master signs both calls, so the winner's balance
// delta must read EXACTLY +0.455 NEAR.

const fs = require("fs");
const nearApi = require("near-api-js");
const { keyStores, KeyPair, Near, Account, utils } = nearApi;

const MASTER = "ghosttech.testnet";
const BOARD = `board.${MASTER}`;
const BULLETIN = `bulletin.${MASTER}`;
const ROASTER = `roaster.${MASTER}`;
const JOB_ID = 4;
const PRICE_NEAR = "3700000";
const GAS = "100000000000000"; // 100 TGas

const lines = [];
function log(msg) {
  console.log(msg);
  lines.push(msg);
}
function appendTranscript() {
  fs.appendFileSync("demo-transcript.txt", lines.join("\n") + "\n");
}
const fmt = (yocto) => utils.format.formatNearAmount(String(yocto));

// encodeArgs / callChecked: same proven pair as demo-run.js — raw-u128
// contract params go on the wire as BARE JSON number literals inside a
// Buffer (the change-call stringifier passes Buffers through untouched).
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

let master;
async function callChecked(contractId, methodName, argsObj) {
  const outcome = await master.functionCall({
    contractId, methodName, args: encodeArgs(argsObj), gas: GAS, attachedDeposit: "0",
  });
  for (const ro of (outcome && outcome.receipts_outcome) || []) {
    for (const l of (ro.outcome && ro.outcome.logs) || []) {
      log(`    Log [${ro.outcome.executor_id}]: ${l}`);
    }
  }
  const status = outcome && outcome.status;
  if (!status || !("SuccessValue" in status)) {
    const detail = status && status.Failure ? JSON.stringify(status.Failure) : JSON.stringify(status);
    throw new Error(`CALL FAILED ${methodName} on ${contractId}: ${detail}`);
  }
  log(`    OK ${methodName} on ${contractId}`);
  return outcome;
}

async function printJob(label) {
  // Raw values only: view-returned u128s arrive as lossy JS numbers
  // past 2^53 and are NEVER fed to fmt() (that is what killed the run).
  const j = await master.viewFunction({ contractId: BOARD, methodName: "get_job", args: { job_id: JOB_ID } });
  log(`${label} job=${JOB_ID} status=${j[0]} currency=${j[1]} budget=${j[2]} best_bid=${j[3] === null ? "none" : j[3]} best_bidder=${j[4] === null ? "none" : j[4]} settlement=${j[5]}`);
  return j;
}

async function main() {
  const priv = process.env.MASTER_PRIVATE_KEY;
  if (!priv) {
    console.error("MASTER_PRIVATE_KEY is not set in this window. Set it first, then re-run.");
    process.exit(1);
  }
  const keyStore = new keyStores.InMemoryKeyStore();
  await keyStore.setKey("testnet", MASTER, KeyPair.fromString(priv));
  const near = new Near({
    networkId: "testnet",
    keyStore,
    nodeUrl: "https://rpc.testnet.fastnear.com",
  });
  master = new Account(near.connection, MASTER);
  const roasterAcc = new Account(near.connection, ROASTER);

  log("");
  log("==============================================================");
  log(`FINISH JOB B — completion pass for the 22:44 demo run (job ${JOB_ID})`);
  log(`Started ${new Date().toISOString()}`);
  log("==============================================================");

  const before = await printJob("STATE");
  if (before[0] !== "Open") {
    throw new Error(`Job ${JOB_ID} is ${before[0]}, not Open — nothing for the finisher to do.`);
  }

  const roasterBefore = BigInt((await roasterAcc.state()).amount);
  const masterBefore = BigInt((await master.state()).amount);
  log(`Balances before finish: roaster = ${fmt(roasterBefore)} NEAR · master = ${fmt(masterBefore)} NEAR`);

  await callChecked(BULLETIN, "post_price", { asset: "NEAR", price: PRICE_NEAR });
  await callChecked(BOARD, "sync_price", { currency: "NEAR", price: PRICE_NEAR });
  log(`PRICE NEAR = ${PRICE_NEAR} micro-USD re-posted and synced (award re-checks freshness)`);

  await callChecked(BOARD, "close_auction", { job_id: JOB_ID });
  log(`AWARD B: close_auction called by poster (expected: status=Awarded)`);
  await printJob("STATE");

  await callChecked(BOARD, "confirm_completion", { job_id: JOB_ID });
  log(`CONFIRM B: poster confirmed completion — escrow releases to the winner (expected: status=Settled)`);
  const final = await printJob("STATE");

  const roasterAfter = BigInt((await roasterAcc.state()).amount);
  const masterAfter = BigInt((await master.state()).amount);
  const dR = roasterAfter - roasterBefore;
  const dM = masterAfter - masterBefore;
  log(`Balances after finish: roaster = ${fmt(roasterAfter)} NEAR · master = ${fmt(masterAfter)} NEAR`);
  log(`SETTLEMENT PROOF B: roaster delta = ${dR >= 0n ? "+" : "-"}${fmt(dR >= 0n ? dR : -dR)} NEAR — exactly the 0.455 NEAR winning bid, released from escrow (the master signed both calls, so the winner spent no gas here).`);
  log(`Master delta = ${dM >= 0n ? "+" : "-"}${fmt(dM >= 0n ? dM : -dM)} NEAR (gas for the finish calls only — its 0.5 NEAR escrow was already locked at posting).`);

  if (final[0] !== "Settled") {
    throw new Error(`Job ${JOB_ID} ended in status ${final[0]}, expected Settled.`);
  }
  if (dR !== 455000000000000000000000n) {
    throw new Error(`Roaster delta is ${dR} yocto, expected exactly 455000000000000000000000.`);
  }
  log(`FINISH DONE: job B (id ${JOB_ID}) settled via real escrow release. The demo is complete: job A (USDC obligation) + job B (NEAR escrow) both settled on-chain.`);
}

main()
  .then(() => appendTranscript())
  .catch((e) => {
    log(`FINISH FAILED: ${e && e.message ? e.message : e}`);
    appendTranscript();
    process.exit(1);
  });
