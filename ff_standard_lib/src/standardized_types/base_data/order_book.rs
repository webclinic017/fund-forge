use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::{Arc};
use chrono::{DateTime, FixedOffset, Utc};
use chrono_tz::Tz;
use futures::future::join_all;
use rkyv::{Archive, Deserialize as Deserialize_rkyv, Serialize as Serialize_rkyv};
use tokio::sync::{RwLock};
use tokio::task;
use crate::helpers::converters::time_local_from_str;
use crate::standardized_types::subscriptions::{Symbol};
use crate::standardized_types::TimeString;

#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, PartialEq)]
#[archive(
    compare(PartialEq),
    check_bytes,
)]
#[archive_attr(derive(Debug))]
pub struct LevelTwoSubscription {
    pub symbol: Symbol,
    pub time: TimeString,
}
impl LevelTwoSubscription {
    pub fn new(symbol: Symbol, time: TimeString) -> Self {

        LevelTwoSubscription {
            symbol,
            time,
        }
    }
}

#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, PartialEq)]
#[archive(
    compare(PartialEq),
    check_bytes,
)]
#[archive_attr(derive(Debug))]
pub struct OrderBookUpdate{
    pub symbol: Symbol,
    pub bid: BTreeMap<u8, f64>,
    pub ask: BTreeMap<u8, f64>,
    time: TimeString,
}

impl OrderBookUpdate {
    pub fn new(symbol: Symbol, bid: BTreeMap<u8, f64>, ask: BTreeMap<u8, f64>, time: DateTime<Utc>) -> Self {
        OrderBookUpdate {
            symbol,
            bid,
            ask,
            time: time.to_string(),
        }
    }

    pub fn time_utc(&self) -> DateTime<chrono::Utc> {
        DateTime::from_str(&self.time).unwrap()
    }

    pub fn time_local(&self, time_zone: &Tz) -> DateTime<FixedOffset> {
        time_local_from_str(time_zone, &self.time)
    }
}


pub struct OrderBook {
    symbol: Symbol,
    bid: Arc<RwLock<BTreeMap<u8, f64>>>, //make it a retain-able history 
    ask: Arc<RwLock<BTreeMap<u8, f64>>>,
    time: Arc<RwLock<DateTime<Utc>>>,
}

impl OrderBook {
    /// Create a new `Price` instance.
    /// # Parameters
    pub fn new(symbol: Symbol, time: DateTime<Utc>) -> Self {
        OrderBook {
            symbol,
            bid: Default::default(),
            ask: Default::default(),
            time: Arc::new(RwLock::new(time)),
        }
    }

    pub async fn update(&self, updates: OrderBookUpdate) {
        if updates.symbol != self.symbol {
            return;
        }
        // Clone necessary fields before moving into the async block
        let updates_bid = updates.bid.clone();
        let updates_ask = updates.ask.clone();
        let updates_time_utc = updates.time_utc();

        let t1 = task::spawn({
            let bid_book = Arc::clone(&self.bid);
            async move {
                let mut bid_book = bid_book.write().await;
                for (level, bid) in updates_bid {
                    bid_book.insert(level, bid);
                }
            }
        });

        let t2 = task::spawn({
            let ask_book = Arc::clone(&self.ask);
            async move {
                let mut ask_book = ask_book.write().await;
                for (level, ask) in updates_ask {
                    ask_book.insert(level, ask);
                }
            }
        });

        let t3 = task::spawn({
            let time = Arc::clone(&self.time);
            async move {
                *time.write().await = updates_time_utc;
            }
        });

        // Wait for all tasks to complete
        join_all(vec![t1, t2, t3]).await;
    }

    pub async fn time_utc(&self) -> DateTime<chrono::Utc> {
        self.time.read().await.clone()
    }

    pub async fn time_local(&self, time_zone: &Tz) -> DateTime<FixedOffset> {
        time_local_from_str(time_zone, &self.time_utc().await.to_string())
    }
    
    pub async fn ask_index(&self, index: u8) -> Option<f64> {
        self.ask.read().await.get(&index).cloned()
    }
    
    pub async fn bid_index(&self, index: u8) -> Option<f64> {
        self.bid.read().await.get(&index).cloned()
    }
    
    pub fn get_ask_book(&self) -> Arc<RwLock<BTreeMap<u8, f64>>> {
        self.ask.clone()
    }

    pub fn get_bid_book(&self) -> Arc<RwLock<BTreeMap<u8, f64>>> {
        self.bid.clone()
    }
}