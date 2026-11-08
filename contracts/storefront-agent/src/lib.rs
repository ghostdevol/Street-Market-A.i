// Storefront Agent — NEAR smart contract
//
// One independently owned storefront per contract instance.
// The contract is the deterministic economic agent.
// The AI face is a separate frontend and has no signing authority.
//
// Currency amounts are u128 in the smallest unit.
// This contract maintains configuration and spending caps.
// It does NOT custody NEAR, USDC, or other tokens.
//
// Recommended near-sdk dependency: 5.x

use near_sdk::{env, near, AccountId, PanicOnDefault};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Pure bidding logic
// ---------------------------------------------------------------------------

/// Calculate the minimum acceptable bid.
///
/// floor = cost + (cost * margin_bps / 10_000)
///
/// Returns None if arithmetic overflows.
pub fn floor_price(cost: u128, margin_bps: u32) -> Option<u128> {
    let margin = cost
        .checked_mul(margin_bps as u128)?
        .checked_div(10_000)?;

    cost.checked_add(margin)
}

/// Calculate the storefront's next bid.
///
/// Rules:
/// - No rival bid: open at asking price if it clears our floor.
/// - Rival above floor: undercut by one step, but never below floor.
/// - Rival at or below floor: refuse to compete.
/// - A zero bid step is treated as one smallest currency unit.
pub fn compute_bid(
    cost: u128,
    margin_bps: u32,
    best_bid: u128,
    step: u128,
    asking: u128,
) -> Option<u128> {
    let floor = floor_price(cost, margin_bps)?;

    // Opening bid.
    if best_bid == 0 {
        return if asking >= floor {
            Some(asking)
        } else {
            None
        };
    }

    // The competitor is already at or below our minimum.
    if best_bid <= floor {
        return None;
    }

    // Undercut without ever crossing our floor.
    let actual_step = step.max(1);
    let candidate = best_bid.saturating_sub(actual_step);

    Some(candidate.max(floor))
}

// ---------------------------------------------------------------------------
// Contract state
// ---------------------------------------------------------------------------

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct StorefrontAgent {
    owner: AccountId,
    shop_name: String,

    // Tasks this storefront is authorized to perform.
    tasks: Vec<String>,

    // Currency symbols accepted by this storefront.
    // These are identifiers, not token integrations.
    currencies: Vec<String>,

    // Item prices and costs, keyed by (currency, item).
    prices: BTreeMap<(String, String), u128>,
    costs: BTreeMap<(String, String), u128>,

    // Minimum profit margin in basis points.
    margin_bps: u32,

    // Amount to undercut a competing bid by.
    bid_step: u128,

    // Maximum authorized spending per currency.
    // This is a policy cap, not an actual token balance.
    spending_caps: BTreeMap<String, u128>,

    jobs_won: u64,
    jobs_done: u64,
}

#[near]
impl StorefrontAgent {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    #[init]
    pub fn new(
        owner: AccountId,
        shop_name: String,
        tasks: Vec<String>,
        currencies: Vec<String>,
        margin_bps: u32,
        bid_step: u128,
    ) -> Self {
        assert_eq!(
            env::predecessor_account_id(),
            owner,
            "Initializer must be the storefront owner"
        );

        assert!(!shop_name.trim().is_empty(), "Shop name required");
        assert!(!tasks.is_empty(), "At least one task required");
        assert!(!currencies.is_empty(), "At least one currency required");

        Self {
            owner,
            shop_name,
            tasks,
            currencies,
            prices: BTreeMap::new(),
            costs: BTreeMap::new(),
            margin_bps,
            bid_step,
            spending_caps: BTreeMap::new(),
            jobs_won: 0,
            jobs_done: 0,
        }
    }

    // -----------------------------------------------------------------------
    // Owner authorization
    // -----------------------------------------------------------------------

    fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the storefront owner can perform this action"
        );
    }

    fn assert_currency_supported(&self, currency: &str) {
        assert!(
            self.currencies.iter().any(|c| c == currency),
            "Currency is not supported by this storefront"
        );
    }

    fn assert_task_supported(&self, task: &str) {
        assert!(
            self.tasks.iter().any(|t| t == task),
            "This storefront is not authorized for that task"
        );
    }

    // -----------------------------------------------------------------------
    // Owner configuration
    // -----------------------------------------------------------------------

    /// Replace the storefront's authorized jobsheet.
    pub fn update_jobsheet(
        &mut self,
        tasks: Vec<String>,
        currencies: Vec<String>,
        margin_bps: u32,
        bid_step: u128,
    ) {
        self.assert_owner();

        assert!(!tasks.is_empty(), "At least one task required");
        assert!(!currencies.is_empty(), "At least one currency required");

        self.tasks = tasks;
        self.currencies = currencies;
        self.margin_bps = margin_bps;
        self.bid_step = bid_step;
    }

    /// Change the storefront display name.
    pub fn set_shop_name(&mut self, shop_name: String) {
        self.assert_owner();

        assert!(!shop_name.trim().is_empty(), "Shop name required");

        self.shop_name = shop_name;
    }

    /// Set an item's price and cost in a supported currency.
    ///
    /// The configured price must never be below the calculated floor.
    pub fn set_item(
        &mut self,
        currency: String,
        item: String,
        price: u128,
        cost: u128,
    ) {
        self.assert_owner();
        self.assert_currency_supported(&currency);

        assert!(!item.trim().is_empty(), "Item name required");

        let floor = floor_price(cost, self.margin_bps)
            .expect("Price floor calculation overflow");

        assert!(
            price >= floor,
            "Configured price cannot be below the minimum floor"
        );

        let key = (currency, item);

        self.prices.insert(key.clone(), price);
        self.costs.insert(key, cost);
    }

    /// Set the maximum spending allowance for a currency.
    ///
    /// This is a policy limit. Actual payment enforcement must be
    /// implemented by the marketplace and asset settlement layer.
    pub fn set_spending_cap(&mut self, currency: String, amount: u128) {
        self.assert_owner();
        self.assert_currency_supported(&currency);

        self.spending_caps.insert(currency, amount);
    }

    // -----------------------------------------------------------------------
    // Deterministic bidding
    // -----------------------------------------------------------------------

    /// Calculate a bid using this storefront's configured item price,
    /// cost, margin, and bid step.
    ///
    /// best_bid must come from a trusted marketplace source.
    /// This method calculates a quote; it does not itself submit a bid.
    pub fn quote_bid(
        &self,
        task: String,
        currency: String,
        item: String,
        best_bid: u128,
    ) -> Option<u128> {
        self.assert_task_supported(&task);
        self.assert_currency_supported(&currency);

        let key = (currency, item);

        let asking = *self.prices.get(&key)?;
        let cost = *self.costs.get(&key)?;

        compute_bid(
            cost,
            self.margin_bps,
            best_bid,
            self.bid_step,
            asking,
        )
    }

    /// Calculate the minimum acceptable price for an item.
    pub fn get_floor(
        &self,
        currency: String,
        item: String,
    ) -> Option<u128> {
        self.assert_currency_supported(&currency);

        let key = (currency, item);
        let cost = *self.costs.get(&key)?;

        floor_price(cost, self.margin_bps)
    }

    // -----------------------------------------------------------------------
    // Read-only storefront information
    // -----------------------------------------------------------------------

    pub fn get_shop_name(&self) -> String {
        self.shop_name.clone()
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    pub fn get_tasks(&self) -> Vec<String> {
        self.tasks.clone()
    }

    pub fn get_currencies(&self) -> Vec<String> {
        self.currencies.clone()
    }

    pub fn get_margin_bps(&self) -> u32 {
        self.margin_bps
    }

    pub fn get_bid_step(&self) -> u128 {
        self.bid_step
    }

    pub fn get_item_price(
        &self,
        currency: String,
        item: String,
    ) -> Option<u128> {
        self.prices.get(&(currency, item)).copied()
    }

    pub fn get_item_cost(
        &self,
        currency: String,
        item: String,
    ) -> Option<u128> {
        self.costs.get(&(currency, item)).copied()
    }

    pub fn get_spending_cap(&self, currency: String) -> Option<u128> {
        self.spending_caps.get(&currency).copied()
    }

    pub fn get_statistics(&self) -> (u64, u64) {
        (self.jobs_won, self.jobs_done)
    }

    // -----------------------------------------------------------------------
    // Owner-attested demo statistics
    // -----------------------------------------------------------------------

    /// These counters are owner-attested in this standalone version.
    /// Production reputation must be updated by verified marketplace
    /// settlement/completion events, not arbitrary owner calls.

    pub fn record_job_won(&mut self) {
        self.assert_owner();

        self.jobs_won = self
            .jobs_won
            .checked_add(1)
            .expect("Job counter overflow");
    }

    pub fn record_job_done(&mut self) {
        self.assert_owner();

        self.jobs_done = self
            .jobs_done
            .checked_add(1)
            .expect("Job counter overflow");

        assert!(
            self.jobs_done <= self.jobs_won,
            "Completed jobs cannot exceed jobs won"
        );
    }
}

// ---------------------------------------------------------------------------
// Host-side unit tests for pure bidding logic
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculates_five_percent_floor() {
        assert_eq!(floor_price(10_000, 500), Some(10_500));
    }

    #[test]
    fn opens_at_asking_price() {
        assert_eq!(
            compute_bid(10_000, 500, 0, 100, 12_000),
            Some(12_000)
        );
    }

    #[test]
    fn refuses_opening_bid_below_floor() {
        assert_eq!(
            compute_bid(10_000, 500, 0, 100, 10_499),
            None
        );
    }

    #[test]
    fn undercuts_competitor_by_one_step() {
        assert_eq!(
            compute_bid(10_000, 500, 12_000, 100, 15_000),
            Some(11_900)
        );
    }

    #[test]
    fn never_bids_below_floor() {
        assert_eq!(
            compute_bid(10_000, 500, 10_550, 500, 15_000),
            Some(10_500)
        );
    }

    #[test]
    fn walks_away_when_competitor_is_at_floor() {
        assert_eq!(
            compute_bid(10_000, 500, 10_500, 100, 15_000),
            None
        );
    }

    #[test]
    fn walks_away_when_competitor_is_below_floor() {
        assert_eq!(
            compute_bid(10_000, 500, 10_000, 100, 15_000),
            None
        );
    }

    #[test]
    fn zero_step_uses_one_unit() {
        assert_eq!(
            compute_bid(10_000, 500, 12_000, 0, 15_000),
            Some(11_999)
        );
    }

    #[test]
    fn detects_floor_overflow() {
        assert_eq!(
            floor_price(u128::MAX, 500),
            None
        );
    }

    #[test]
    fn detects_margin_multiplication_overflow() {
        assert_eq!(
            floor_price(u128::MAX / 2, u32::MAX),
            None
        );
    }
}
