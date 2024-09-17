use std::collections::BTreeMap;
use chrono::{DateTime, Utc};
use crate::standardized_types::accounts::ledgers::{AccountId, Ledger};
use crate::standardized_types::base_data::order_book::{OrderBook, OrderBookUpdate};
use crate::standardized_types::enums::{OrderSide};
use crate::standardized_types::orders::orders::{Order, OrderRequest, OrderUpdateEvent};
use crate::standardized_types::strategy_events::{EventTimeSlice, StrategyEvent};
use crate::standardized_types::subscriptions::SymbolName;
use crate::standardized_types::{Price};
use std::sync::Arc;
use dashmap::DashMap;
use futures::future::join_all;
use tokio::sync::mpsc::{Receiver};
use tokio::sync::{RwLock};
use crate::apis::brokerage::broker_enum::Brokerage;
use crate::market_handler::historical::order_matching;
use crate::server_connections::send_strategy_event_slice;
use crate::standardized_types::base_data::base_data_enum::BaseDataEnum;
use crate::standardized_types::time_slices::TimeSlice;

pub struct MarketHandler {
    order_books: Arc<DashMap<SymbolName, Arc<OrderBook>>>,
    last_price: Arc<DashMap<SymbolName, Price>>,
    ledgers: Arc<DashMap<Brokerage, Arc<DashMap<AccountId, Ledger>>>>,
    last_time: Arc<RwLock<DateTime<Utc>>>,
    order_cache: Arc<RwLock<Vec<Order>>>,
}

impl MarketHandler {
    pub async fn new(
        start_time: DateTime<Utc>,
        order_receiver: Option<Receiver<OrderRequest>>,
    ) -> Self {
        let handler = Self {
            order_books: Arc::new(DashMap::new()),
            last_price: Arc::new(DashMap::new()),
            ledgers: Arc::new(DashMap::new()),
            last_time: Arc::new(RwLock::new(start_time)),
            order_cache: Arc::new(RwLock::new(Vec::new())),
        };

        if let Some(order_receiver) = order_receiver {
            handler.simulated_order_matching(order_receiver).await;
        }
        handler
    }

    pub async fn get_order_book(&self, symbol_name: &SymbolName) -> Option<Arc<OrderBook>> {
        if let Some(book) = self.order_books.get(symbol_name) {
            return Some(book.clone());
        }
        None
    }

    pub async fn get_last_price(&self, symbol_name: &SymbolName) -> Option<Price> {
        if let Some(price) = self.last_price.get(symbol_name) {
            return Some(price.value().clone());
        }
        None
    }

    pub async fn get_pending_orders(&self) -> Vec<Order> {
        self.order_cache.read().await.clone()
    }

    pub async fn get_last_time(&self) -> DateTime<Utc> {
        self.last_time.read().await.clone()
    }

    pub fn export_trades(&self, path: &str) {
        for broker_map in self.ledgers.iter() {
            for ledger in broker_map.iter() {
                ledger.value().export_positions_to_csv(&path);
            }
        }
    }

    pub async fn update_time_slice(&self, time: DateTime<Utc>, time_slice: TimeSlice) {
        {
            *self.last_time.write().await = time;
        }
        let last_price = self.last_price.clone();
        let order_books = self.order_books.clone();
        let mut updates = vec![];
        for base_data in time_slice {
            let last_price_ref = last_price.clone();
            let order_book_ref = order_books.clone();
            let update_future = tokio::spawn(async move {
                match base_data {
                    BaseDataEnum::TradePrice(ref price) => {
                        last_price_ref.insert(price.symbol.name.clone(), price.price);
                    }
                    BaseDataEnum::Candle(ref candle) => {
                        last_price_ref.insert(candle.symbol.name.clone(), candle.close);
                    }
                    BaseDataEnum::QuoteBar(ref bar) => {
                        if !order_book_ref.contains_key(&bar.symbol.name) {
                            order_book_ref.insert(
                                bar.symbol.name.clone(),
                                Arc::new(OrderBook::new(bar.symbol.clone(), bar.time_utc())),
                            );
                        }
                        if let Some(book) = order_book_ref.get_mut(&bar.symbol.name) {
                            let mut bid = BTreeMap::new();
                            bid.insert(0, bar.bid_close.clone());
                            let mut ask = BTreeMap::new();
                            ask.insert(0, bar.ask_close.clone());
                            let order_book_update =
                                OrderBookUpdate::new(bar.symbol.clone(), bid, ask, bar.time_utc());
                            book.update(order_book_update).await;
                        }
                    }
                    BaseDataEnum::Tick(ref tick) => {
                        last_price_ref.insert(tick.symbol.name.clone(), tick.price);
                    }
                    BaseDataEnum::Quote(ref quote) => {
                        if !order_book_ref.contains_key(&quote.symbol.name) {
                            order_book_ref.insert(
                                quote.symbol.name.clone(),
                                Arc::new(OrderBook::new(quote.symbol.clone(), quote.time_utc())),
                            );
                        }
                        if let Some(book) = order_book_ref.get_mut(&quote.symbol.name) {
                            let mut bid = BTreeMap::new();
                            bid.insert(quote.book_level.clone(), quote.bid.clone());
                            let mut ask = BTreeMap::new();
                            ask.insert(quote.book_level.clone(), quote.ask.clone());
                            let order_book_update =
                                OrderBookUpdate::new(quote.symbol.clone(), bid, ask, quote.time_utc());
                            book.update(order_book_update).await;
                        }
                    }
                    BaseDataEnum::Fundamental(_) => (),
                }
            });
           updates.push(update_future);
        }
        join_all(updates).await;
    }

    pub async fn simulated_order_matching(
        &self,
        mut order_receiver: Receiver<OrderRequest>,
    ) {
        let mut event_buffer = Arc::new(RwLock::new(EventTimeSlice::new()));
        let last_time = self.last_time.clone();
        let ledgers = self.ledgers.clone();
        let last_price = self.last_price.clone();
        let order_cache = self.order_cache.clone();
        let order_books = self.order_books.clone();
        let mut order_receiver = order_receiver;
        tokio::task::spawn(async move {
            while let Some(order_request) = order_receiver.recv().await {
                let last_time = last_time.read().await;
                match order_request {
                    OrderRequest::Create { brokerage, order } => {
                        order_cache.write().await.push(order);
                        match order_matching::backtest_matching_engine(order_books.clone(), last_price.clone(), ledgers.clone(), last_time.clone(), order_cache.clone()).await {
                            None => {},
                            Some(event) => event_buffer.write().await.extend(event)
                        }
                    }
                    OrderRequest::Cancel{brokerage, order_id } => {
                        let mut existing_order: Option<Order> = None;
                        let mut cache = order_cache.write().await;
                        'order_search: for order in &*cache {
                            if order.id == order_id {
                                existing_order = Some(order.clone());
                                break 'order_search;
                            }
                        }
                        if let Some(order) = existing_order {
                            cache.retain(|x| x.id != order_id);
                            let cancel_event = StrategyEvent::OrderEvents(OrderUpdateEvent::Cancelled(order.id));
                            event_buffer.write().await.push(cancel_event);
                        } else {
                            let fail_event = StrategyEvent::OrderEvents(OrderUpdateEvent::UpdateRejected { id: order_id, reason: String::from("No pending order found") });
                            event_buffer.write().await.push(fail_event);
                        }
                    }
                    OrderRequest::Update{ brokerage, order_id, order } => {
                        let mut existing_order: Option<Order> = None;
                        let mut cache = order_cache.write().await;
                        'order_search: for order in &*cache {
                            if order.id == order_id {
                                existing_order = Some(order.clone());
                                break 'order_search;
                            }
                        }
                        if let Some(_) = existing_order {
                            cache.retain(|x| x.id != order_id);
                            let update_event = StrategyEvent::OrderEvents(OrderUpdateEvent::Updated(order.id.clone()));
                            cache.push(order);
                            event_buffer.write().await.push(update_event);
                        } else {
                            let fail_event = StrategyEvent::OrderEvents(OrderUpdateEvent::UpdateRejected { id: order_id, reason: String::from("No pending order found") });
                            event_buffer.write().await.push(fail_event);
                        }
                        match order_matching::backtest_matching_engine(order_books.clone(), last_price.clone(), ledgers.clone(), last_time.clone(), order_cache.clone()).await {
                            None => {},
                            Some(event) => event_buffer.write().await.extend(event)
                        }
                    }
                }
                let mut event_buffer = event_buffer.write().await;
                if !event_buffer.is_empty() {
                    send_strategy_event_slice(event_buffer.clone()).await;
                    event_buffer.clear();
                }
            }
        });
    }

    pub async fn process_ledgers(&self) -> Vec<String> {
        // Acquire a read lock on the RwLock
        // Iterate over the HashMap while holding the read lock
        let mut return_strings = vec![];
        for brokerage_map in self.ledgers.iter() {
            for ledger in brokerage_map.iter() {
                return_strings.push(format!("{} \n", ledger.value().print()));
            }
        }
        return_strings
    }

    pub async fn is_long(&self, brokerage: &Brokerage, account_id: &AccountId, symbol_name: &SymbolName) -> bool {
        if let Some(ledger_map) = self.ledgers.get(brokerage) {
            if let Some(broker_ledgers) = ledger_map.get(account_id) {
                return broker_ledgers.value().is_long(symbol_name).await
            }
            return false
        }
        false
    }

    pub async fn is_short(&self, brokerage: &Brokerage, account_id: &AccountId, symbol_name: &SymbolName) -> bool {
        if let Some(ledger_map) = self.ledgers.get(brokerage) {
            if let Some(broker_ledgers) = ledger_map.get(account_id) {
                return broker_ledgers.value().is_short(symbol_name).await
            }
            return false
        }
        false
    }

    pub async fn is_flat(&self, brokerage: &Brokerage, account_id: &AccountId, symbol_name: &SymbolName) -> bool {
        if let Some(ledger_map) = self.ledgers.get(brokerage) {
            if let Some(broker_ledgers) = ledger_map.get(account_id) {
                return broker_ledgers.value().is_flat(symbol_name).await
            }
            return true
        }
        true
    }
}

pub async fn get_market_price(
    order_side: &OrderSide,
    symbol_name: &SymbolName,
    order_books: Arc<DashMap<SymbolName, Arc<OrderBook>>>,
    last_price: Arc<DashMap<SymbolName, Price>>,
) -> Result<Price, String> {

    if let Some(book) = order_books.get(symbol_name) {
        match order_side {
            OrderSide::Buy => {
                if let Some(ask_price) = book.ask_level(0).await {
                    return Ok(ask_price);
                }
            }
            OrderSide::Sell => {
                if let Some(bid_price) = book.bid_level(0).await {
                    return Ok(bid_price);
                }
            }
        }
    } else if let Some(last_price) = last_price.get(symbol_name) {
        return Ok(last_price.value().clone());
    }
    Err(String::from("No market price found for symbol"))
}

