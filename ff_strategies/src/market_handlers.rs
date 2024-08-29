use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc};
use chrono::{DateTime, Utc};
use tokio::sync::{RwLock};
use ff_standard_lib::apis::brokerage::Brokerage;
use ff_standard_lib::standardized_types::accounts::ledgers::{AccountId, Ledger};
use ff_standard_lib::standardized_types::base_data::base_data_enum::BaseDataEnum;
use ff_standard_lib::standardized_types::enums::{OrderSide, StrategyMode};
use ff_standard_lib::standardized_types::orders::orders::{Order, OrderState, OrderType, OrderUpdateEvent};
use ff_standard_lib::standardized_types::OwnerId;
use ff_standard_lib::standardized_types::subscriptions::Symbol;
use ff_standard_lib::standardized_types::time_slices::TimeSlice;
use ff_standard_lib::standardized_types::strategy_events::{EventTimeSlice, StrategyEvent};
use ahash::AHashMap;
use ff_standard_lib::standardized_types::base_data::order_book::{OrderBook, OrderBookUpdate};
/*lazy_static! LIVE_MARKET_EVENT_HANDLER {

}*/



pub(crate) enum MarketHandlerEnum {
    BacktestDefault(HistoricalMarketHandler),
    LiveDefault(Arc<HistoricalMarketHandler>) //ToDo Will use this later so that live strategies share an event handler.. the local platform will also use this event handler
}

impl MarketHandlerEnum {
    pub fn new(owner_id: OwnerId, start_time: DateTime<Utc>, mode: StrategyMode) -> Self {
        match mode {
            StrategyMode::Backtest => MarketHandlerEnum::BacktestDefault(HistoricalMarketHandler::new(owner_id, start_time)),
            _ => panic!("Live mode not supported yet"),
        }
    }

    pub(crate) async fn last_time(&self) -> DateTime<Utc> {
        match self {
            MarketHandlerEnum::BacktestDefault(handler) => handler.last_time.read().await.clone(),
            MarketHandlerEnum::LiveDefault(_) => panic!("Live mode should not use this function for time")
        }
    }

    pub(crate) async fn update_time(&self, time: DateTime<Utc>) {
        match self {
            MarketHandlerEnum::BacktestDefault(handler) => handler.update_time(time).await,
            MarketHandlerEnum::LiveDefault(_) => panic!("Live mode should not use this function for time")
        }
    }

    //Updates the market handler with incoming data and returns any triggered events
    // TimeSlice: A collection of base data to update the handler with
    pub async fn base_data_upate(&self, time_slice: &TimeSlice) -> Option<Vec<StrategyEvent>> {
        match self {
            MarketHandlerEnum::BacktestDefault(handler) => handler.on_data_update(time_slice).await,
            MarketHandlerEnum::LiveDefault(handler) => handler.on_data_update(time_slice).await //ToDo we will need a historical handler just to warm up the strategy
        }
    }

    pub async fn process_ledgers(&self) {
        match self {
            MarketHandlerEnum::BacktestDefault(handler) => handler.process_ledgers().await,
            MarketHandlerEnum::LiveDefault(handler) => handler.process_ledgers().await
        }
    }

    pub async fn send_order(&self, order: Order) {
        match self {
            MarketHandlerEnum::BacktestDefault(handler) => handler.send_order(order).await,
            MarketHandlerEnum::LiveDefault(handler) => handler.send_order(order).await
        }
    }
    
    pub async fn set_warm_up_complete(&self) {
        match self {
            MarketHandlerEnum::BacktestDefault(handler) => handler.set_warm_up_complete().await,
            MarketHandlerEnum::LiveDefault(handler) => handler.set_warm_up_complete().await
        }
    }
}

pub(crate) struct HistoricalMarketHandler {
    owner_id: String,
    /// The strategy receives its timeslices using strategy events, this is for other processes that need time slices and do not need synchronisation with the strategy
    /// These time slices are sent before they are sent to the strategy as events
    order_books: RwLock<AHashMap<Symbol, OrderBook>>,
    last_price: RwLock<AHashMap<Symbol, f64>>,
    ledgers: RwLock<AHashMap<Brokerage, HashMap<AccountId, Ledger>>>,
    pub(crate) last_time: RwLock<DateTime<Utc>>,
    pub order_cache: RwLock<Vec<Order>>,
    warm_up_complete: RwLock<bool>,
}

impl HistoricalMarketHandler {
   pub(crate) fn new(owner_id: OwnerId, start_time: DateTime<Utc>) -> Self {
        Self {
            owner_id,
            order_books: Default::default(),
            last_price: Default::default(),
            ledgers: Default::default(),
            last_time: RwLock::new(start_time),
            order_cache: Default::default(),
            warm_up_complete: RwLock::new(false),
        }
    }

    pub async fn send_order(&self, order: Order) {
        self.order_cache.write().await.push(order);
    }

    pub async fn update_time(&self, time: DateTime<Utc>) {
        *self.last_time.write().await = time;
    }
    
    pub async fn set_warm_up_complete(&self) {
        *self.warm_up_complete.write().await = true;
    }

    /// only primary data gets to here, so we can update our order books etc
    pub(crate) async fn on_data_update(&self, time_slice: &TimeSlice) -> Option<EventTimeSlice> {
        for ledger in self.ledgers.read().await.values() {
            for ledger in ledger.values() {
                ledger.on_data_update(time_slice.clone()).await;
            }
        }
        for base_data in time_slice {
            //println!("Base data: {:?}", base_data.time_created_utc());
            match base_data {
                BaseDataEnum::Price(ref price) => {
                    let mut last_price = self.last_price.write().await;
                    last_price.insert(price.symbol.clone(), price.price);
                }
                BaseDataEnum::Candle(ref candle) => {
                    let mut last_price = self.last_price.write().await;
                    last_price.insert(candle.symbol.clone(), candle.close);
                }
                BaseDataEnum::QuoteBar(ref bar) => {
                    
                }
                BaseDataEnum::Tick(ref tick) => {
                    let mut last_price = self.last_price.write().await;
                    last_price.insert(tick.symbol.clone(), tick.price);
                }
                BaseDataEnum::Quote(ref quote) => {
                    let mut order_books = self.order_books.write().await;
                    if !order_books.contains_key(&quote.symbol) {
                        order_books.insert(quote.symbol.clone(), OrderBook::new(quote.symbol.clone(), quote.time_utc()));
                    }
                    if let Some(book) = order_books.get_mut(&quote.symbol) {
                        let mut bid = BTreeMap::new();
                        bid.insert(quote.book_level.clone(), quote.bid.clone());
                        let mut ask = BTreeMap::new();
                        ask.insert(quote.book_level.clone(), quote.ask.clone());
                        let order_book_update = OrderBookUpdate::new(quote.symbol.clone(), bid, ask, quote.time_utc());
                        book.update(order_book_update).await;
                    }
                },
                BaseDataEnum::Fundamental(_) => ()
            }
        }
        if *self.warm_up_complete.read().await {
            self.backtest_matching_engine().await
        } else {
            None
        }
    }

    pub(crate) async fn backtest_matching_engine(&self) -> Option<EventTimeSlice> {
        let mut orders = self.order_cache.write().await;
        if orders.len() == 0 {
            return None;
        }

        let mut events = Vec::new();
        let orders = &mut *orders;
        let remaining_orders: Vec<Order> = Vec::new();
        for order in &mut *orders {
            //1. If we don't have a brokerage + account create one
            if !self.ledgers.read().await.contains_key(&order.brokerage) {
                self.ledgers.write().await.insert(order.brokerage.clone(), HashMap::new());
            }
            if !self.ledgers.read().await.get(&order.brokerage).unwrap().contains_key(&order.account_id) {
                let ledger = order.brokerage.paper_ledger(order.account_id.clone(), order.brokerage.user_currency(), order.brokerage.starting_cash()).await;
                self.ledgers.write().await.get_mut(&order.brokerage).unwrap().insert(order.account_id.clone(), ledger);
            }

            //2. send the order to the ledger to be handled
            let mut ledgers = self.ledgers.write().await;
            let ledger = ledgers.get_mut(&order.brokerage).unwrap().get_mut(&order.account_id).unwrap();

            let owner_id = self.owner_id.clone();

            //3. respond with an order event
            match order.order_type {
                OrderType::Limit => {
                    // todo! for limit orders that are not hit we need to push the order into remaining orders, so that it can be retained
                    panic!("Limit orders not supported in backtest")
                },
                OrderType::Market => panic!("Market orders not supported in backtest"),
                OrderType::MarketIfTouched => panic!("MarketIfTouched orders not supported in backtest"),
                OrderType::StopMarket => panic!("StopMarket orders not supported in backtest"),
                OrderType::StopLimit => panic!("StopLimit orders not supported in backtest"),
                OrderType::TakeProfit => panic!("TakeProfit orders not supported in backtest"),
                OrderType::StopLoss => panic!("StopLoss orders not supported in backtest"),
                OrderType::GuaranteedStopLoss => panic!("GuaranteedStopLoss orders not supported in backtest"),
                OrderType::TrailingStopLoss => panic!("TrailingStopLoss orders not supported in backtest"),
                OrderType::TrailingGuaranteedStopLoss => panic!("TrailingGuaranteedStopLoss orders not supported in backtest"),
                OrderType::EnterLong => {
                    if ledger.is_short(&order.symbol).await {
                        ledger.exit_position_paper(&order.symbol).await;
                    }
                    let market_price = self.get_market_price(&OrderSide::Buy, &order.symbol).await.unwrap();
                    match ledger.enter_long_paper(order.symbol.clone(), order.quantity_ordered.clone(), market_price).await {
                        Ok(_) => {
                            order.state = OrderState::Filled;
                            order.average_fill_price = Some(market_price);
                            events.push(StrategyEvent::OrderEvents(owner_id.clone(), OrderUpdateEvent::Filled(order.clone())));
                        },
                        Err(e) => {
                            let reason = format!("Failed to enter long position: {:?}", e);
                            order.state = OrderState::Rejected(reason);
                            order.time_created_utc = self.last_time.read().await.to_string();
                            events.push(StrategyEvent::OrderEvents(owner_id.clone(), OrderUpdateEvent::Rejected(order.clone())));
                        }
                    }
                },
                OrderType::EnterShort => {
                    if ledger.is_long(&order.symbol).await {
                        ledger.exit_position_paper(&order.symbol).await;
                    }
                    let market_price = self.get_market_price(&OrderSide::Buy, &order.symbol).await.unwrap();
                    match ledger.enter_short_paper(order.symbol.clone(), order.quantity_ordered.clone(), market_price).await {
                        Ok(_) => {
                            order.state = OrderState::Filled;
                            order.average_fill_price = Some(market_price);
                            events.push(StrategyEvent::OrderEvents(owner_id.clone(), OrderUpdateEvent::Filled(order.clone())));
                        },
                        Err(e) => {
                            let reason = format!("Failed to enter short position: {:?}", e);
                            order.state = OrderState::Rejected(reason);
                            order.time_created_utc = self.last_time.read().await.to_string();
                            events.push(StrategyEvent::OrderEvents(owner_id.clone(), OrderUpdateEvent::Rejected(order.clone())));
                        }
                    }
                },
                OrderType::ExitLong => {
                    if ledger.is_long(&order.symbol).await {
                        order.state = OrderState::Filled;
                        let market_price = self.get_market_price(&OrderSide::Buy, &order.symbol).await.unwrap();
                        order.average_fill_price = Some(market_price);
                        events.push(StrategyEvent::OrderEvents(owner_id.clone(), OrderUpdateEvent::Filled(order.clone())));

                    } else {
                        let reason = "No long position to exit".to_string();
                        order.state = OrderState::Rejected(reason);
                        order.time_created_utc = self.last_time.read().await.to_string();
                        events.push(StrategyEvent::OrderEvents(owner_id.clone(), OrderUpdateEvent::Rejected(order.clone())));
                    }
                },
                OrderType::ExitShort => {
                    if ledger.is_short(&order.symbol).await {
                        order.state = OrderState::Filled;
                        let market_price = self.get_market_price(&OrderSide::Buy, &order.symbol).await.unwrap();
                        order.average_fill_price = Some(market_price);
                        events.push(StrategyEvent::OrderEvents(owner_id.clone(), OrderUpdateEvent::Filled(order.clone())));
                    } else {
                        let reason = "No short position to exit".to_string();
                        order.state = OrderState::Rejected(reason);
                        order.time_created_utc = self.last_time.read().await.to_string();
                        events.push(StrategyEvent::OrderEvents(owner_id.clone(), OrderUpdateEvent::Rejected(order.clone())));
                    }
                },
            };
        };
        *orders = remaining_orders;
        if events.len() == 0 {
            None
        } else {
            Some(events)
        }
    }

    pub(crate) async fn process_ledgers(&self) {
        // Acquire a read lock on the RwLock
        let ledgers = self.ledgers.read().await;

        // Iterate over the HashMap while holding the read lock
        for (brokerage, accounts) in ledgers.iter() {
            for (account_id, ledger) in accounts.iter() {
                println!("Brokerage: {:?} Account: {:?} Ledger: {:?}", brokerage, account_id, ledger);
            }
        }
    }

    async fn get_market_price(&self, order_side: &OrderSide, symbol_name: &Symbol) -> Result<f64, MarketPriceError> {
        if let Some(book) = self.order_books.read().await.get(symbol_name) {
            match order_side {
                OrderSide::Buy => {
                    if let Some(ask_price) = book.ask_index(0).await {
                        return Ok(ask_price)
                    }
                },
                OrderSide::Sell => {
                    if let Some(bid_price) = book.bid_index(0).await {
                        return Ok(bid_price)
                    } 
                },
            }
        } else if let Some(last_price) = self.last_price.read().await.get(symbol_name) {
            return Ok(last_price.clone())
        }
        Err(MarketPriceError::NoPriceFound("No market price found for symbol".into()))
    }
}

#[derive(Debug)]
pub enum MarketPriceError {
    NoPriceFound(String),
}

