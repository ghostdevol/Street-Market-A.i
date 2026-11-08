// Street Board — NEAR smart contract (StreetMarket AI marketplace)
//
// The street itself. Five modules, per Daniel's spec (2026-11-08):
//
//   1. Job Registry   — publish jobs, requirements, rewards
//   2. Auction Engine — collect and order competing bids
//   3. Bid Validation — check storefront authorization and price freshness
//   4. Award Engine   — select the winning eligible bid
//   5. Settlement     — lock and release payment after verified completion
//
// Auction form: reverse auction. Shops bid DOWN the price they will
// charge; the lowest eligible bid wins.
//
// ---------------------------------------------------------------------------
// Settlement law (hardened 2026-11-08, Daniel's review):
//
// NEAR jobs and non-NEAR jobs are different animals and are never
// reported the same way:
//
// - NEAR jobs escrow REAL NEAR in this contract at posting. Release on
//   confirmed completion moves actual funds. Status Settled means PAID.
//
// - Non-NEAR jobs (USDC, BTC, ZEC...) create an OBLIGATION at award.
//   An obligation is a recorded debt, NOT secured funds, and this
//   contract never calls it settled. Its lifecycle is explicit:
//       Pending -> Fulfilled   (settlement layer reports the asset moved,
//                               with a reference: tx id / intent id)
//               -> Failed      (settlement layer reports it did not move)
//               -> Expired     (settle window passed with no fulfillment)
//   The poster's confirm_completion on a non-NEAR job records verified
//   work ONLY (status AwaitingSettlement). The job becomes Settled
//   exclusively when the obligation is Fulfilled.
//
// Price freshness gates exactly two moments: accepting a bid, and
// awarding. It does NOT gate settlement: escrowed NEAR is real money
// whose release owes nothing to a price feed, and a fulfillment report
// is an external fact. If the price is stale at award time, the award
// REFUSES (the job stays open) rather than expiring a won deal over a
// lagging keeper — sync the price and close again.
//
// Poster ghosting: if the poster never confirms completion, the winner
// may claim after the completion window (deemed acceptance). For NEAR
// jobs the escrow releases; for non-NEAR jobs the job advances to
// AwaitingSettlement with its obligation still Pending.

use near_sdk::{env, near, AccountId, NearToken, PanicOnDefault, Promise};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[near(serializers = [borsh])]
#[derive(Clone, PartialEq, Debug)]
pub enum JobStatus {
    Open,
    Awarded,
    /// Work confirmed; a non-NEAR obligation is still unsettled.
    AwaitingSettlement,
    Settled,
    SettlementFailed,
    Cancelled,
    Expired,
}

#[near(serializers = [borsh])]
#[derive(Clone, PartialEq, Debug)]
pub enum SettlementState {
    /// NEAR job, or not yet awarded: no obligation exists.
    None,
    Pending,
    Fulfilled,
    Failed,
    Expired,
}

#[near(serializers = [borsh])]
#[derive(Clone)]
pub struct Job {
    poster: AccountId,
    task: String,
    item: String,
    currency: String,
    /// Maximum the poster will pay (the reward cap).
    budget: u128,
    deadline_ns: u64,
    status: JobStatus,
    best_bid: Option<u128>,
    best_bidder: Option<AccountId>,
    /// NEAR jobs: yocto locked in this contract. Non-NEAR jobs: the
    /// obligation amount recorded at award (kept as the record even if
    /// the obligation fails or expires; zeroed only when Fulfilled).
    escrow: u128,
    settlement: SettlementState,
    /// Evidence reference for a fulfilled obligation (tx id / intent id).
    settlement_ref: Option<String>,
    awarded_ns: u64,
}

#[near(serializers = [borsh])]
pub struct StorefrontEntry {
    tasks: Vec<String>,
    currencies: Vec<String>,
}

#[near(serializers = [borsh])]
pub struct PricePoint {
    price: u128,
    posted_ns: u64,
}

// ---------------------------------------------------------------------------
// Contract state
// ---------------------------------------------------------------------------

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct StreetBoard {
    owner: AccountId,
    keeper: AccountId,
    max_price_age_ns: u64,
    /// How long the poster has to confirm completion before the winner
    /// may claim by timeout (deemed acceptance).
    completion_window_ns: u64,
    /// How long a non-NEAR obligation may stay Pending before it can be
    /// expired.
    settle_window_ns: u64,
    next_job_id: u64,
    jobs: BTreeMap<u64, Job>,
    storefronts: BTreeMap<AccountId, StorefrontEntry>,
    prices: BTreeMap<String, PricePoint>,
    /// Ledger mirror of RELEASED payments only (winner -> total). An
    /// unfulfilled obligation never appears here.
    paid_out: BTreeMap<AccountId, u128>,
}

#[near]
impl StreetBoard {
    #[init]
    pub fn new(
        owner: AccountId,
        keeper: AccountId,
        max_price_age_seconds: u64,
        completion_window_seconds: u64,
        settle_window_seconds: u64,
    ) -> Self {
        assert_eq!(
            env::predecessor_account_id(),
            owner,
            "Initializer must be the board owner"
        );
        assert!(max_price_age_seconds > 0, "Max price age must be positive");
        assert!(completion_window_seconds > 0, "Completion window must be positive");
        assert!(settle_window_seconds > 0, "Settle window must be positive");
        Self {
            owner,
            keeper,
            max_price_age_ns: max_price_age_seconds * 1_000_000_000,
            completion_window_ns: completion_window_seconds * 1_000_000_000,
            settle_window_ns: settle_window_seconds * 1_000_000_000,
            next_job_id: 1,
            jobs: BTreeMap::new(),
            storefronts: BTreeMap::new(),
            prices: BTreeMap::new(),
            paid_out: BTreeMap::new(),
        }
    }

    fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the board owner can perform this action"
        );
    }

    fn assert_keeper_or_poster(&self, job: &Job) {
        let caller = env::predecessor_account_id();
        assert!(
            caller == self.keeper || caller == self.owner || caller == job.poster,
            "Only the keeper, owner, or poster can report settlement"
        );
    }

    fn price_fresh(&self, currency: &str) -> bool {
        match self.prices.get(currency) {
            Some(p) => {
                env::block_timestamp().saturating_sub(p.posted_ns) <= self.max_price_age_ns
            }
            None => false,
        }
    }

    fn release_near(&mut self, job_id: u64) {
        // Shared by confirm_completion and claim_after_timeout on NEAR jobs.
        let job = self.jobs.get(&job_id).unwrap().clone();
        let winner = job.best_bidder.clone().expect("Awarded job has a winner");
        let amount = job.best_bid.expect("Awarded job has a winning bid");
        let job = self.jobs.get_mut(&job_id).unwrap();
        job.status = JobStatus::Settled;
        job.escrow = 0;
        Promise::new(winner.clone()).transfer(NearToken::from_yoctonear(amount));
        let total = self.paid_out.get(&winner).copied().unwrap_or(0) + amount;
        self.paid_out.insert(winner.clone(), total);
        env::log_str(&format!(
            "SETTLED job={} winner={} amount={}",
            job_id, winner, amount
        ));
    }

    // -----------------------------------------------------------------------
    // Keeper: price sync (module 3's freshness source)
    // -----------------------------------------------------------------------

    pub fn sync_price(&mut self, currency: String, price: u128) {
        let caller = env::predecessor_account_id();
        assert!(
            caller == self.keeper || caller == self.owner,
            "Only the authorized keeper can sync prices"
        );
        assert!(price > 0, "Price must be positive");
        self.prices.insert(
            currency,
            PricePoint { price, posted_ns: env::block_timestamp() },
        );
    }

    // -----------------------------------------------------------------------
    // Storefront registry (module 3's authorization source)
    // -----------------------------------------------------------------------

    pub fn register_storefront(&mut self, tasks: Vec<String>, currencies: Vec<String>) {
        assert!(!tasks.is_empty(), "At least one task required");
        assert!(!currencies.is_empty(), "At least one currency required");
        let who = env::predecessor_account_id();
        self.storefronts.insert(who, StorefrontEntry { tasks, currencies });
    }

    pub fn deregister_storefront(&mut self) {
        let who = env::predecessor_account_id();
        self.storefronts.remove(&who);
    }

    pub fn is_registered(&self, account: AccountId) -> bool {
        self.storefronts.contains_key(&account)
    }

    // -----------------------------------------------------------------------
    // Module 1 — Job Registry
    // -----------------------------------------------------------------------

    /// Publish a job. For NEAR jobs the full budget must be attached: it
    /// locks in escrow at birth (module 5's lock, up front).
    #[payable]
    pub fn post_job(
        &mut self,
        task: String,
        item: String,
        currency: String,
        budget: u128,
        duration_seconds: u64,
    ) -> u64 {
        assert!(!task.trim().is_empty(), "Task required");
        assert!(!item.trim().is_empty(), "Item required");
        assert!(budget > 0, "Budget must be positive");
        assert!(duration_seconds > 0, "Duration must be positive");

        let attached = env::attached_deposit().as_yoctonear();
        let escrow = if currency == "NEAR" {
            assert_eq!(
                attached, budget,
                "NEAR jobs lock the full budget as escrow at posting"
            );
            attached
        } else {
            assert_eq!(attached, 0, "Attach NEAR only to NEAR-denominated jobs");
            0
        };

        let id = self.next_job_id;
        self.next_job_id += 1;
        let deadline_ns = env::block_timestamp() + duration_seconds * 1_000_000_000;
        self.jobs.insert(
            id,
            Job {
                poster: env::predecessor_account_id(),
                task,
                item,
                currency,
                budget,
                deadline_ns,
                status: JobStatus::Open,
                best_bid: None,
                best_bidder: None,
                escrow,
                settlement: SettlementState::None,
                settlement_ref: None,
                awarded_ns: 0,
            },
        );
        env::log_str(&format!("JOB_POSTED id={} budget={}", id, budget));
        id
    }

    // -----------------------------------------------------------------------
    // Modules 2 + 3 — Auction Engine with Bid Validation
    // -----------------------------------------------------------------------

    pub fn bid(&mut self, job_id: u64, amount: u128) {
        let bidder = env::predecessor_account_id();
        let job = self.jobs.get(&job_id).expect("Unknown job").clone();

        assert!(job.status == JobStatus::Open, "Job is not open");
        assert!(env::block_timestamp() <= job.deadline_ns, "Auction has ended");
        let entry = self
            .storefronts
            .get(&bidder)
            .expect("Bidder is not a registered storefront");
        assert!(
            entry.tasks.iter().any(|t| t == &job.task),
            "Storefront is not authorized for this task"
        );
        assert!(
            entry.currencies.iter().any(|c| c == &job.currency),
            "Storefront does not list this job's currency"
        );
        assert!(
            self.price_fresh(&job.currency),
            "Price for {} is stale — the board does not trade on old prices",
            job.currency
        );
        assert!(amount > 0, "Bid must be positive");
        assert!(amount <= job.budget, "Bid exceeds the job budget");
        if let Some(best) = job.best_bid {
            assert!(amount < best, "Bid must beat the current best bid");
        }

        let job = self.jobs.get_mut(&job_id).unwrap();
        job.best_bid = Some(amount);
        job.best_bidder = Some(bidder.clone());
        env::log_str(&format!(
            "BID job={} bidder={} amount={}",
            job_id, bidder, amount
        ));
    }

    // -----------------------------------------------------------------------
    // Module 4 — Award Engine
    // -----------------------------------------------------------------------

    /// Close the auction and award. The poster may close any time; anyone
    /// may close after the deadline. Freshness is re-checked at award: a
    /// stale price REFUSES the award (job stays open) rather than killing
    /// a won deal over a lagging keeper. A missing or deregistered winner
    /// expires the job and refunds any escrow.
    pub fn close_auction(&mut self, job_id: u64) {
        let caller = env::predecessor_account_id();
        let job = self.jobs.get(&job_id).expect("Unknown job").clone();
        assert!(job.status == JobStatus::Open, "Job is not open");
        assert!(
            caller == job.poster || env::block_timestamp() > job.deadline_ns,
            "Only the poster can close before the deadline"
        );

        match (&job.best_bidder, job.best_bid) {
            (Some(winner), Some(amount)) => {
                assert!(
                    self.price_fresh(&job.currency),
                    "Price for {} is stale — cannot award on old prices; sync and retry",
                    job.currency
                );
                if !self.storefronts.contains_key(winner) {
                    self.expire_and_refund(job_id);
                    return;
                }
                let winner = winner.clone();
                let job = self.jobs.get_mut(&job_id).unwrap();
                job.status = JobStatus::Awarded;
                job.awarded_ns = env::block_timestamp();
                if job.currency != "NEAR" {
                    // The obligation exists from this moment. It is a
                    // recorded debt — not funds, not a settlement.
                    job.escrow = amount;
                    job.settlement = SettlementState::Pending;
                }
                env::log_str(&format!(
                    "AWARD job={} winner={} amount={}",
                    job_id, winner, amount
                ));
            }
            _ => self.expire_and_refund(job_id),
        }
    }

    fn expire_and_refund(&mut self, job_id: u64) {
        let job = self.jobs.get(&job_id).unwrap().clone();
        let refund = job.escrow;
        let poster = job.poster.clone();
        let job = self.jobs.get_mut(&job_id).unwrap();
        job.status = JobStatus::Expired;
        job.escrow = 0;
        if refund > 0 {
            Promise::new(poster).transfer(NearToken::from_yoctonear(refund));
        }
        env::log_str(&format!("EXPIRED job={} refund={}", job_id, refund));
    }

    // -----------------------------------------------------------------------
    // Module 5 — Settlement
    // -----------------------------------------------------------------------

    /// The poster confirms verified completion.
    /// - NEAR job: the escrowed funds release; the job is Settled (paid).
    /// - Non-NEAR job: this records verified work ONLY. The job moves to
    ///   AwaitingSettlement with its obligation still Pending. No funds
    ///   have moved and nothing here claims otherwise.
    pub fn confirm_completion(&mut self, job_id: u64) {
        let caller = env::predecessor_account_id();
        let job = self.jobs.get(&job_id).expect("Unknown job").clone();
        assert!(job.status == JobStatus::Awarded, "Job is not awarded");
        assert_eq!(caller, job.poster, "Only the poster confirms completion");

        if job.currency == "NEAR" {
            self.release_near(job_id);
        } else {
            let job = self.jobs.get_mut(&job_id).unwrap();
            job.status = JobStatus::AwaitingSettlement;
            env::log_str(&format!(
                "COMPLETION_CONFIRMED job={} settlement=PENDING (obligation, no funds moved by this contract)",
                job_id
            ));
        }
    }

    /// The winner claims after the completion window when the poster
    /// never confirmed (deemed acceptance). NEAR escrow releases; a
    /// non-NEAR job advances to AwaitingSettlement like a confirmation.
    pub fn claim_after_timeout(&mut self, job_id: u64) {
        let caller = env::predecessor_account_id();
        let job = self.jobs.get(&job_id).expect("Unknown job").clone();
        assert!(job.status == JobStatus::Awarded, "Job is not awarded");
        assert_eq!(
            Some(&caller),
            job.best_bidder.as_ref(),
            "Only the winning storefront can claim by timeout"
        );
        assert!(
            env::block_timestamp() > job.awarded_ns + self.completion_window_ns,
            "Completion window has not passed — the poster still has time to confirm"
        );

        if job.currency == "NEAR" {
            self.release_near(job_id);
        } else {
            let job = self.jobs.get_mut(&job_id).unwrap();
            job.status = JobStatus::AwaitingSettlement;
            env::log_str(&format!(
                "COMPLETION_DEEMED job={} settlement=PENDING (poster never confirmed)",
                job_id
            ));
        }
    }

    /// Settlement layer reports the obligation's asset actually moved.
    /// Only now may a non-NEAR job be called Settled.
    pub fn report_settlement_fulfilled(&mut self, job_id: u64, reference: String) {
        let job = self.jobs.get(&job_id).expect("Unknown job").clone();
        self.assert_keeper_or_poster(&job);
        assert!(
            job.status == JobStatus::AwaitingSettlement,
            "Job is not awaiting settlement"
        );
        assert!(!reference.trim().is_empty(), "Settlement reference required");

        let winner = job.best_bidder.clone().expect("Awarded job has a winner");
        let amount = job.escrow;
        let job = self.jobs.get_mut(&job_id).unwrap();
        job.settlement = SettlementState::Fulfilled;
        job.settlement_ref = Some(reference.clone());
        job.status = JobStatus::Settled;
        job.escrow = 0;
        let total = self.paid_out.get(&winner).copied().unwrap_or(0) + amount;
        self.paid_out.insert(winner.clone(), total);
        env::log_str(&format!(
            "SETTLEMENT_FULFILLED job={} winner={} amount={} ref={}",
            job_id, winner, amount, reference
        ));
    }

    /// Settlement layer reports the obligation did not move.
    pub fn report_settlement_failed(&mut self, job_id: u64, reason: String) {
        let job = self.jobs.get(&job_id).expect("Unknown job").clone();
        self.assert_keeper_or_poster(&job);
        assert!(
            job.status == JobStatus::AwaitingSettlement,
            "Job is not awaiting settlement"
        );
        let job = self.jobs.get_mut(&job_id).unwrap();
        job.settlement = SettlementState::Failed;
        job.status = JobStatus::SettlementFailed;
        // escrow keeps the failed obligation amount as the record.
        env::log_str(&format!(
            "SETTLEMENT_FAILED job={} reason={}",
            job_id, reason
        ));
    }

    /// Anyone may expire an obligation that outlived its settle window.
    pub fn expire_settlement(&mut self, job_id: u64) {
        let job = self.jobs.get(&job_id).expect("Unknown job").clone();
        assert!(
            job.status == JobStatus::AwaitingSettlement,
            "Job is not awaiting settlement"
        );
        assert!(
            env::block_timestamp() > job.awarded_ns + self.settle_window_ns,
            "Settle window has not passed"
        );
        let job = self.jobs.get_mut(&job_id).unwrap();
        job.settlement = SettlementState::Expired;
        job.status = JobStatus::Expired;
        env::log_str(&format!("SETTLEMENT_EXPIRED job={}", job_id));
    }

    /// Poster cancels an open job before any award; escrow returns.
    pub fn cancel_job(&mut self, job_id: u64) {
        let caller = env::predecessor_account_id();
        let job = self.jobs.get(&job_id).expect("Unknown job").clone();
        assert!(job.status == JobStatus::Open, "Only an open job can be cancelled");
        assert_eq!(caller, job.poster, "Only the poster can cancel");

        let refund = job.escrow;
        let job = self.jobs.get_mut(&job_id).unwrap();
        job.status = JobStatus::Cancelled;
        job.escrow = 0;
        if refund > 0 {
            Promise::new(caller).transfer(NearToken::from_yoctonear(refund));
        }
        env::log_str(&format!("CANCELLED job={} refund={}", job_id, refund));
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// (status, currency, budget, best_bid, best_bidder, settlement_state)
    pub fn get_job(
        &self,
        job_id: u64,
    ) -> Option<(String, String, u128, Option<u128>, Option<AccountId>, String)> {
        self.jobs.get(&job_id).map(|j| {
            (
                format!("{:?}", j.status),
                j.currency.clone(),
                j.budget,
                j.best_bid,
                j.best_bidder.clone(),
                format!("{:?}", j.settlement),
            )
        })
    }

    pub fn job_escrow(&self, job_id: u64) -> u128 {
        self.jobs.get(&job_id).map(|j| j.escrow).unwrap_or(0)
    }

    pub fn settlement_ref(&self, job_id: u64) -> Option<String> {
        self.jobs.get(&job_id).and_then(|j| j.settlement_ref.clone())
    }

    /// Released payments only. Pending obligations never appear here.
    pub fn paid_out_to(&self, account: AccountId) -> u128 {
        self.paid_out.get(&account).copied().unwrap_or(0)
    }

    pub fn job_count(&self) -> u64 {
        self.next_job_id - 1
    }
}

// ---------------------------------------------------------------------------
// Tests — the whole street, walked end to end, incl. Daniel's two
// scrutiny cases from 2026-11-08: staleness between bid and settlement,
// and the poster who never confirms.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    const SEC: u64 = 1_000_000_000;
    const NEAR: u128 = 1_000_000_000_000_000_000_000_000; // 1 NEAR in yocto

    fn ctx(who: AccountId, ts_ns: u64, deposit: u128) -> VMContextBuilder {
        let mut c = VMContextBuilder::new();
        c.predecessor_account_id(who);
        c.block_timestamp(ts_ns);
        c.attached_deposit(NearToken::from_yoctonear(deposit));
        c
    }

    /// Board: price max age 120s, completion window 3600s, settle window
    /// 7200s. accounts(0)=owner/keeper, accounts(1)=poster,
    /// accounts(2)=shop A, accounts(3)=shop B.
    fn street() -> StreetBoard {
        testing_env!(ctx(accounts(0), 100 * SEC, 0).build());
        let mut b = StreetBoard::new(accounts(0), accounts(0), 120, 3600, 7200);
        b.sync_price("USDC".into(), 1_000_000);
        b.sync_price("NEAR".into(), 3_700_000);
        testing_env!(ctx(accounts(2), 100 * SEC, 0).build());
        b.register_storefront(vec!["deliver".into()], vec!["USDC".into(), "NEAR".into()]);
        testing_env!(ctx(accounts(3), 100 * SEC, 0).build());
        b.register_storefront(vec!["deliver".into()], vec!["USDC".into(), "NEAR".into()]);
        b
    }

    fn post_usdc_job(b: &mut StreetBoard) -> u64 {
        testing_env!(ctx(accounts(1), 100 * SEC, 0).build());
        b.post_job("deliver".into(), "coffee".into(), "USDC".into(), 900, 600)
    }

    fn status(b: &StreetBoard, id: u64) -> String {
        b.get_job(id).unwrap().0
    }

    fn settlement(b: &StreetBoard, id: u64) -> String {
        b.get_job(id).unwrap().5
    }

    #[test]
    fn obligation_is_not_funds_until_fulfilled() {
        let mut b = street();
        let id = post_usdc_job(&mut b);
        testing_env!(ctx(accounts(2), 100 * SEC, 0).build());
        b.bid(id, 880);
        testing_env!(ctx(accounts(3), 100 * SEC, 0).build());
        b.bid(id, 850);
        testing_env!(ctx(accounts(1), 100 * SEC, 0).build());
        b.close_auction(id);
        assert_eq!(settlement(&b, id), "Pending");
        assert_eq!(b.job_escrow(id), 850); // the recorded obligation

        // Poster confirms the work. NOTHING is paid yet — by design.
        b.confirm_completion(id);
        assert_eq!(status(&b, id), "AwaitingSettlement");
        assert_eq!(b.paid_out_to(accounts(3)), 0);

        // The settlement layer reports the asset moved, with evidence.
        b.report_settlement_fulfilled(id, "intent-0xabc".into());
        assert_eq!(status(&b, id), "Settled");
        assert_eq!(settlement(&b, id), "Fulfilled");
        assert_eq!(b.settlement_ref(id), Some("intent-0xabc".to_string()));
        assert_eq!(b.paid_out_to(accounts(3)), 850);
        assert_eq!(b.job_escrow(id), 0);
    }

    #[test]
    fn obligation_can_fail_and_can_expire() {
        // Failed path.
        let mut b = street();
        let id = post_usdc_job(&mut b);
        testing_env!(ctx(accounts(2), 100 * SEC, 0).build());
        b.bid(id, 800);
        testing_env!(ctx(accounts(1), 100 * SEC, 0).build());
        b.close_auction(id);
        b.confirm_completion(id);
        b.report_settlement_failed(id, "intent expired unFilled".into());
        assert_eq!(status(&b, id), "SettlementFailed");
        assert_eq!(settlement(&b, id), "Failed");
        assert_eq!(b.paid_out_to(accounts(2)), 0);
        assert_eq!(b.job_escrow(id), 800); // failed amount kept as the record

        // Expired path: settle window is 7200s from award at t=100s.
        let mut b = street();
        let id = post_usdc_job(&mut b);
        testing_env!(ctx(accounts(2), 100 * SEC, 0).build());
        b.bid(id, 800);
        testing_env!(ctx(accounts(1), 100 * SEC, 0).build());
        b.close_auction(id);
        b.confirm_completion(id);
        testing_env!(ctx(accounts(4), (100 + 7201) * SEC, 0).build());
        b.expire_settlement(id);
        assert_eq!(status(&b, id), "Expired");
        assert_eq!(settlement(&b, id), "Expired");
        assert_eq!(b.paid_out_to(accounts(2)), 0);
    }

    #[test]
    fn near_job_locks_real_escrow_and_releases_it() {
        let mut b = street();
        testing_env!(ctx(accounts(1), 100 * SEC, 5 * NEAR).build());
        let id = b.post_job("deliver".into(), "coffee".into(), "NEAR".into(), 5 * NEAR, 600);
        assert_eq!(b.job_escrow(id), 5 * NEAR);
        testing_env!(ctx(accounts(2), 100 * SEC, 0).build());
        b.bid(id, 4 * NEAR);
        testing_env!(ctx(accounts(1), 100 * SEC, 0).build());
        b.close_auction(id);
        b.confirm_completion(id);
        assert_eq!(status(&b, id), "Settled");
        assert_eq!(b.job_escrow(id), 0);
        assert_eq!(b.paid_out_to(accounts(2)), 4 * NEAR);
    }

    #[test]
    fn stale_price_at_award_refuses_then_recovers() {
        let mut b = street();
        let id = post_usdc_job(&mut b); // posted t=100, deadline t=700
        testing_env!(ctx(accounts(2), 100 * SEC, 0).build());
        b.bid(id, 800);

        // t=300: price (synced t=100, max age 120) is stale.
        testing_env!(ctx(accounts(1), 300 * SEC, 0).build());
        let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            b.close_auction(id);
        }));
        assert!(refused.is_err(), "award on stale prices must refuse");
        assert_eq!(status(&b, id), "Open"); // the deal is NOT dead

        // Keeper syncs; the same close now awards.
        testing_env!(ctx(accounts(0), 300 * SEC, 0).build());
        b.sync_price("USDC".into(), 1_000_000);
        testing_env!(ctx(accounts(1), 300 * SEC, 0).build());
        b.close_auction(id);
        assert_eq!(status(&b, id), "Awarded");
    }

    #[test]
    fn stale_price_after_award_does_not_block_near_settlement() {
        let mut b = street();
        testing_env!(ctx(accounts(1), 100 * SEC, 5 * NEAR).build());
        let id = b.post_job("deliver".into(), "coffee".into(), "NEAR".into(), 5 * NEAR, 600);
        testing_env!(ctx(accounts(2), 100 * SEC, 0).build());
        b.bid(id, 4 * NEAR);
        testing_env!(ctx(accounts(1), 100 * SEC, 0).build());
        b.close_auction(id);
        // Far future: every price is ancient. Escrowed money doesn't care.
        testing_env!(ctx(accounts(1), 100_000 * SEC, 0).build());
        b.confirm_completion(id);
        assert_eq!(status(&b, id), "Settled");
        assert_eq!(b.paid_out_to(accounts(2)), 4 * NEAR);
    }

    #[test]
    fn poster_never_confirms_winner_claims_after_window() {
        let mut b = street();
        testing_env!(ctx(accounts(1), 100 * SEC, 5 * NEAR).build());
        let id = b.post_job("deliver".into(), "coffee".into(), "NEAR".into(), 5 * NEAR, 600);
        testing_env!(ctx(accounts(2), 100 * SEC, 0).build());
        b.bid(id, 4 * NEAR);
        testing_env!(ctx(accounts(1), 100 * SEC, 0).build());
        b.close_auction(id); // awarded at t=100; window 3600s

        // Too early: the poster still has its window.
        testing_env!(ctx(accounts(2), (100 + 3599) * SEC, 0).build());
        let early = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            b.claim_after_timeout(id);
        }));
        assert!(early.is_err());

        // Window passed, poster silent: the winner collects the escrow.
        testing_env!(ctx(accounts(2), (100 + 3601) * SEC, 0).build());
        b.claim_after_timeout(id);
        assert_eq!(status(&b, id), "Settled");
        assert_eq!(b.paid_out_to(accounts(2)), 4 * NEAR);
        assert_eq!(b.job_escrow(id), 0);
    }

    #[test]
    #[should_panic(expected = "not a registered storefront")]
    fn unregistered_bidder_is_rejected() {
        let mut b = street();
        let id = post_usdc_job(&mut b);
        testing_env!(ctx(accounts(4), 100 * SEC, 0).build());
        b.bid(id, 100);
    }

    #[test]
    #[should_panic(expected = "not authorized for this task")]
    fn wrong_task_storefront_is_rejected() {
        let mut b = street();
        let id = post_usdc_job(&mut b);
        testing_env!(ctx(accounts(4), 100 * SEC, 0).build());
        b.register_storefront(vec!["repair".into()], vec!["USDC".into()]);
        b.bid(id, 100);
    }

    #[test]
    #[should_panic(expected = "stale")]
    fn stale_price_blocks_bidding() {
        let mut b = street();
        let id = post_usdc_job(&mut b);
        testing_env!(ctx(accounts(2), 300 * SEC, 0).build());
        b.bid(id, 100);
    }

    #[test]
    #[should_panic(expected = "must beat the current best bid")]
    fn non_improving_bid_is_rejected() {
        let mut b = street();
        let id = post_usdc_job(&mut b);
        testing_env!(ctx(accounts(2), 100 * SEC, 0).build());
        b.bid(id, 850);
        testing_env!(ctx(accounts(3), 100 * SEC, 0).build());
        b.bid(id, 860);
    }

    #[test]
    fn no_bid_job_expires_and_refunds() {
        let mut b = street();
        testing_env!(ctx(accounts(1), 100 * SEC, 5 * NEAR).build());
        let id = b.post_job("deliver".into(), "coffee".into(), "NEAR".into(), 5 * NEAR, 60);
        testing_env!(ctx(accounts(4), 200 * SEC, 0).build());
        b.close_auction(id);
        assert_eq!(status(&b, id), "Expired");
        assert_eq!(b.job_escrow(id), 0);
    }

    #[test]
    fn poster_cancels_open_job() {
        let mut b = street();
        let id = post_usdc_job(&mut b);
        testing_env!(ctx(accounts(1), 100 * SEC, 0).build());
        b.cancel_job(id);
        assert_eq!(status(&b, id), "Cancelled");
    }

    #[test]
    #[should_panic(expected = "Only the poster confirms")]
    fn stranger_cannot_confirm_completion() {
        let mut b = street();
        let id = post_usdc_job(&mut b);
        testing_env!(ctx(accounts(2), 100 * SEC, 0).build());
        b.bid(id, 800);
        testing_env!(ctx(accounts(1), 100 * SEC, 0).build());
        b.close_auction(id);
        testing_env!(ctx(accounts(3), 100 * SEC, 0).build());
        b.confirm_completion(id);
    }
}
