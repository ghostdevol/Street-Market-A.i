//! StreetMarket simulation — three storefront agents, one posted job,
//! a live bid war run through the REAL contract code (quote_bid).
//!
//! Market driver rules (the board's job, outside the agent contract):
//! - Shops take turns quoting against the current best bid.
//! - A shop never bids above its own listed price (driver caps the quote).
//! - The current leader doesn't bid against itself.
//! - The war ends when a full round passes with no improvement.

use near_sdk::test_utils::{accounts, VMContextBuilder};
use near_sdk::testing_env;
use storefront_agent::StorefrontAgent;

fn shop(name: &str, price: u128, cost: u128, margin_bps: u32, step: u128) -> StorefrontAgent {
    let mut s = StorefrontAgent::new(
        accounts(0),
        name.to_string(),
        vec!["sell".to_string(), "deliver".to_string()],
        vec!["USDC".to_string()],
        margin_bps,
        step,
    );
    s.set_item("USDC".into(), "coffee".into(), price, cost);
    s
}

#[test]
fn three_shops_one_job_bid_war() {
    let mut ctx = VMContextBuilder::new();
    ctx.predecessor_account_id(accounts(0));
    testing_env!(ctx.build());

    // name, list price, cost, margin bps, bid step -> floors: 810 / 759 / 864
    let mut shops = vec![
        shop("corner-cafe", 900, 770, 520, 10),
        shop("roaster-supply", 880, 730, 400, 15),
        shop("night-owl", 950, 800, 800, 5),
    ];

    println!("POST job=coffee-run task=deliver item=coffee currency=USDC asking=900");
    for s in &shops {
        println!(
            "DESK {} floor={}",
            s.get_shop_name(),
            s.get_floor("USDC".into(), "coffee".into()).unwrap()
        );
    }

    let mut best: u128 = 0;
    let mut leader: Option<usize> = None;

    for _round in 0..100 {
        let mut improved = false;
        for i in 0..shops.len() {
            if leader == Some(i) {
                continue; // never bid against yourself
            }
            let s = &shops[i];
            let quote = s.quote_bid("deliver".into(), "USDC".into(), "coffee".into(), best);
            match quote {
                Some(q) => {
                    let asking = s.get_item_price("USDC".into(), "coffee".into()).unwrap();
                    let effective = if best == 0 { q } else { q.min(asking) };
                    if best == 0 || effective < best {
                        best = effective;
                        leader = Some(i);
                        improved = true;
                        println!("BID {} {}", s.get_shop_name(), effective);
                    } else {
                        println!("HOLD {} quote {} not under best {}", s.get_shop_name(), effective, best);
                    }
                }
                None => {
                    println!(
                        "PASS {} (floor {})",
                        s.get_shop_name(),
                        s.get_floor("USDC".into(), "coffee".into()).unwrap()
                    );
                }
            }
        }
        if !improved {
            break;
        }
    }

    let w = leader.expect("someone must win");
    let winner = &mut shops[w];
    let cost = winner.get_item_cost("USDC".into(), "coffee".into()).unwrap();
    let floor = winner.get_floor("USDC".into(), "coffee".into()).unwrap();
    winner.record_job_won();
    winner.record_job_done();
    println!(
        "WIN {} @ {} — cost {} floor {} keeps {} over cost; record {:?}",
        winner.get_shop_name(),
        best,
        cost,
        floor,
        best - cost,
        winner.get_statistics()
    );

    assert!(best >= floor, "winner settled below its floor — impossible by rule");
}
