// Price Bulletin — NEAR smart contract
//
// The street's chalkboard. One shared board of live prices that every
// storefront reads, so all agents price, bid, and value their treasuries
// against the same numbers at the same moment.
//
// Prices are u128 in micro-USD (1_000_000 = $1.00).
//
// Honest mechanics: a blockchain cannot fetch prices from the internet.
// This contract is the board; an authorized off-chain keeper holds the
// chalk, posting prices from a real feed on a short interval. The board
// defends itself with a staleness rule: any price older than max_age is
// flagged, and the fresh-only reads refuse it. A dead keeper gives you a
// visibly dead board — never silently wrong prices.

use near_sdk::{env, near, AccountId, PanicOnDefault};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Pure conversion logic
// ---------------------------------------------------------------------------

/// Convert an amount between two assets at bulletin prices.
///
/// value = amount * from_price / to_price
///
/// Returns None on overflow or a zero target price.
pub fn convert(amount: u128, from_price: u128, to_price: u128) -> Option<u128> {
    amount.checked_mul(from_price)?.checked_div(to_price)
}

// ---------------------------------------------------------------------------
// Contract state
// ---------------------------------------------------------------------------

#[near(serializers = [borsh])]
pub struct PricePoint {
    /// Price in micro-USD.
    price: u128,
    /// Block timestamp (nanoseconds) when this price was posted.
    posted_ns: u64,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct PriceBulletin {
    owner: AccountId,

    // The keeper account allowed to post prices.
    updater: AccountId,

    // A price older than this many nanoseconds is stale.
    max_age_ns: u64,

    // Asset symbol -> latest posted price.
    prices: BTreeMap<String, PricePoint>,
}

#[near]
impl PriceBulletin {
    #[init]
    pub fn new(owner: AccountId, updater: AccountId, max_age_seconds: u64) -> Self {
        assert_eq!(
            env::predecessor_account_id(),
            owner,
            "Initializer must be the bulletin owner"
        );
        assert!(max_age_seconds > 0, "Max age must be positive");

        Self {
            owner,
            updater,
            max_age_ns: max_age_seconds * 1_000_000_000,
            prices: BTreeMap::new(),
        }
    }

    fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the bulletin owner can perform this action"
        );
    }

    fn assert_updater(&self) {
        let caller = env::predecessor_account_id();
        assert!(
            caller == self.updater || caller == self.owner,
            "Only the authorized keeper can post prices"
        );
    }

    fn is_fresh(&self, point: &PricePoint) -> bool {
        env::block_timestamp().saturating_sub(point.posted_ns) <= self.max_age_ns
    }

    // -----------------------------------------------------------------------
    // Keeper writes
    // -----------------------------------------------------------------------

    /// Post the latest price for an asset. Keeper (or owner) only.
    pub fn post_price(&mut self, asset: String, price: u128) {
        self.assert_updater();
        assert!(!asset.trim().is_empty(), "Asset symbol required");
        assert!(price > 0, "Price must be positive");

        let posted_ns = env::block_timestamp();
        self.prices.insert(asset.clone(), PricePoint { price, posted_ns });
        env::log_str(&format!("PRICE asset={} price={} at={}", asset, price, posted_ns));
    }

    // -----------------------------------------------------------------------
    // Owner configuration
    // -----------------------------------------------------------------------

    pub fn set_updater(&mut self, updater: AccountId) {
        self.assert_owner();
        self.updater = updater;
    }

    pub fn set_max_age_seconds(&mut self, max_age_seconds: u64) {
        self.assert_owner();
        assert!(max_age_seconds > 0, "Max age must be positive");
        self.max_age_ns = max_age_seconds * 1_000_000_000;
    }

    // -----------------------------------------------------------------------
    // Reads — what the storefront contracts utilize
    // -----------------------------------------------------------------------

    /// Latest price for an asset: (price, posted_ns, is_fresh).
    /// Stale prices are returned WITH the stale flag — callers decide.
    pub fn get_price(&self, asset: String) -> Option<(u128, u64, bool)> {
        self.prices.get(&asset).map(|p| (p.price, p.posted_ns, self.is_fresh(p)))
    }

    /// Fresh-only price. Returns None if the asset is unknown OR its
    /// price has gone stale. This is the read trading logic should use.
    pub fn get_price_fresh(&self, asset: String) -> Option<u128> {
        let p = self.prices.get(&asset)?;
        if self.is_fresh(p) {
            Some(p.price)
        } else {
            None
        }
    }

    /// Convert an amount between two assets at fresh bulletin prices.
    /// None if either price is missing/stale or the math overflows.
    pub fn convert_at_bulletin(
        &self,
        amount: u128,
        from_asset: String,
        to_asset: String,
    ) -> Option<u128> {
        let from_price = self.get_price_fresh(from_asset)?;
        let to_price = self.get_price_fresh(to_asset)?;
        convert(amount, from_price, to_price)
    }

    pub fn assets(&self) -> Vec<String> {
        self.prices.keys().cloned().collect()
    }

    pub fn get_updater(&self) -> AccountId {
        self.updater.clone()
    }

    pub fn get_max_age_seconds(&self) -> u64 {
        self.max_age_ns / 1_000_000_000
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    const SEC: u64 = 1_000_000_000;

    fn ctx_at(who: AccountId, ts_ns: u64) -> VMContextBuilder {
        let mut c = VMContextBuilder::new();
        c.predecessor_account_id(who);
        c.block_timestamp(ts_ns);
        c
    }

    fn board() -> PriceBulletin {
        testing_env!(ctx_at(accounts(0), 100 * SEC).build());
        PriceBulletin::new(accounts(0), accounts(1), 60)
    }

    #[test]
    fn convert_math() {
        assert_eq!(convert(100, 3_000, 2), Some(150_000));
        assert_eq!(convert(2, 50_000, 25_000), Some(4));
        assert_eq!(convert(100, 5, 0), None); // zero target price
        assert_eq!(convert(u128::MAX, 2, 1), None); // overflow
    }

    #[test]
    fn keeper_posts_and_shops_read_fresh() {
        let mut b = board();
        testing_env!(ctx_at(accounts(1), 100 * SEC).build()); // keeper posts
        b.post_price("NEAR".into(), 3_700_000);
        b.post_price("USDC".into(), 1_000_000);

        assert_eq!(b.get_price_fresh("NEAR".into()), Some(3_700_000));
        // 10 NEAR at $3.70 = 37 USDC (same unit scale both sides)
        assert_eq!(
            b.convert_at_bulletin(10, "NEAR".into(), "USDC".into()),
            Some(37)
        );
        assert_eq!(b.assets(), vec!["NEAR".to_string(), "USDC".to_string()]);
    }

    #[test]
    fn stale_price_is_flagged_and_refused() {
        let mut b = board();
        testing_env!(ctx_at(accounts(1), 100 * SEC).build());
        b.post_price("NEAR".into(), 3_700_000);

        // 61 seconds later, max age is 60: flagged stale, fresh read refuses.
        testing_env!(ctx_at(accounts(1), 161 * SEC).build());
        let (price, _ts, fresh) = b.get_price("NEAR".into()).unwrap();
        assert_eq!(price, 3_700_000);
        assert!(!fresh);
        assert_eq!(b.get_price_fresh("NEAR".into()), None);
        assert_eq!(b.convert_at_bulletin(10, "NEAR".into(), "NEAR".into()), None);
    }

    #[test]
    #[should_panic(expected = "Only the authorized keeper")]
    fn stranger_cannot_post_prices() {
        let mut b = board();
        testing_env!(ctx_at(accounts(2), 100 * SEC).build());
        b.post_price("NEAR".into(), 1);
    }

    #[test]
    #[should_panic(expected = "Only the bulletin owner")]
    fn keeper_cannot_replace_itself() {
        let mut b = board();
        testing_env!(ctx_at(accounts(1), 100 * SEC).build());
        b.set_updater(accounts(2));
    }
}
