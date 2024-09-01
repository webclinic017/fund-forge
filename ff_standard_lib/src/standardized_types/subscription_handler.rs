use crate::apis::vendor::client_requests::ClientSideDataVendor;
use crate::consolidators::consolidator_enum::ConsolidatorEnum;
use crate::standardized_types::base_data::base_data_enum::BaseDataEnum;
use crate::standardized_types::base_data::base_data_type::BaseDataType;
use crate::standardized_types::data_server_messaging::FundForgeError;
use crate::standardized_types::enums::{Resolution, StrategyMode, SubscriptionResolutionType};
use crate::standardized_types::rolling_window::RollingWindow;
use crate::standardized_types::subscriptions;
use crate::standardized_types::subscriptions::{DataSubscription, DataSubscriptionEvent, Symbol};
use crate::standardized_types::time_slices::TimeSlice;
use ahash::AHashMap;
use chrono::{DateTime, Utc};
use futures::future::join_all;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Manages all subscriptions for a strategy. each strategy has its own subscription handler.
pub struct SubscriptionHandler {
    /// Manages the subscriptions of specific symbols
    symbol_subscriptions: Arc<RwLock<AHashMap<Symbol, SymbolSubscriptionHandler>>>,
    fundamental_subscriptions: RwLock<Vec<DataSubscription>>,
    /// Keeps a record when the strategy has updated its subscriptions, so we can pause the backtest to fetch new data.
    subscriptions_updated: RwLock<bool>,
    is_warmed_up: Mutex<bool>,
    strategy_mode: StrategyMode,
    // subscriptions which the strategy actually subscribed to, not the raw data needed to fullfill the subscription.
    strategy_subscriptions: RwLock<Vec<DataSubscription>>,
}

impl SubscriptionHandler {
    pub async fn new(strategy_mode: StrategyMode) -> Self {
        SubscriptionHandler {
            fundamental_subscriptions: Default::default(),
            symbol_subscriptions: Default::default(),
            subscriptions_updated: RwLock::new(true),
            is_warmed_up: Mutex::new(false),
            strategy_mode,
            strategy_subscriptions: Default::default(),
        }
    }

    /// Sets the SubscriptionHandler as warmed up, so we can start processing data.
    /// This lets the handler know that it needs to manually warm up any future subscriptions.
    pub async fn set_warmup_complete(&self) {
        *self.is_warmed_up.lock().await = true;
        for symbol_handler in self.symbol_subscriptions.write().await.values_mut() {
            symbol_handler.set_warmed_up();
        }
    }

    /// Returns all the subscription events that have occurred since the last time this method was called.
    pub async fn subscription_events(&self) -> Vec<DataSubscriptionEvent> {
        let mut subscription_events = vec![];
        for symbol_handler in self.symbol_subscriptions.write().await.values_mut() {
            subscription_events.extend(symbol_handler.get_subscription_event_buffer());
        }
        subscription_events
    }

    pub async fn strategy_subscriptions(&self) -> Vec<DataSubscription> {
        let strategy_subscriptions = self.strategy_subscriptions.read().await;
        strategy_subscriptions.clone()
    }

    /// Subscribes to a new data subscription
    /// 'new_subscription: DataSubscription' The new subscription to subscribe to.
    /// 'history_to_retain: usize' The number of bars to retain in the history.
    /// 'current_time: DateTime<Utc>' The current time is used to warm up consolidator history if we have already done our initial strategy warm up.
    /// 'strategy_mode: StrategyMode' The strategy mode is used to determine how to warm up the history, in live mode we may not yet have a serialized history to the current time.
    pub async fn subscribe(
        &self,
        new_subscription: DataSubscription,
        history_to_retain: u64,
        current_time: DateTime<Utc>,
    ) -> Result<(), FundForgeError> {
        if new_subscription.base_data_type == BaseDataType::Fundamentals {
            //subscribe to fundamental
            let mut fundamental_subscriptions = self.fundamental_subscriptions.write().await;
            if !fundamental_subscriptions.contains(&new_subscription) {
                fundamental_subscriptions.push(new_subscription.clone());
            }
            let mut strategy_subscriptions = self.strategy_subscriptions.write().await;
            if !strategy_subscriptions.contains(&new_subscription) {
                strategy_subscriptions.push(new_subscription.clone());
            }
            *self.subscriptions_updated.write().await = true;
            return Ok(());
        }
        let mut symbol_subscriptions = self.symbol_subscriptions.write().await;
        if !symbol_subscriptions.contains_key(&new_subscription.symbol) {
            let symbol_handler = SymbolSubscriptionHandler::new(
                new_subscription.clone(),
                self.is_warmed_up.lock().await.clone(),
                history_to_retain,
                current_time,
                self.strategy_mode,
            )
            .await;
            symbol_subscriptions.insert(new_subscription.symbol.clone(), symbol_handler);
        }
        let mut strategy_subscriptions = self.strategy_subscriptions.write().await;
        if !strategy_subscriptions.contains(&new_subscription) {
            strategy_subscriptions.push(new_subscription.clone());
        }
        let symbol_handler = symbol_subscriptions
            .get_mut(&new_subscription.symbol)
            .unwrap();
        symbol_handler
            .subscribe(
                new_subscription,
                history_to_retain,
                current_time,
                self.strategy_mode,
            )
            .await;

        *self.subscriptions_updated.write().await = true;
        Ok(())
    }

    /// Unsubscribes from a data subscription
    /// 'subscription: DataSubscription' The subscription to unsubscribe from.
    /// 'current_time: DateTime<Utc>' The current time is used to change our base data subscription and warm up any new consolidators if we are adjusting our base resolution.
    /// 'strategy_mode: StrategyMode' The strategy mode is used to determine how to warm up the history, in live mode we may not yet have a serialized history to the current time.
    pub async fn unsubscribe(&self, subscription: DataSubscription) -> Result<(), FundForgeError> {
        if subscription.base_data_type == BaseDataType::Fundamentals {
            let mut fundamental_subscriptions = self.fundamental_subscriptions.write().await;
            if fundamental_subscriptions.contains(&subscription) {
                fundamental_subscriptions
                    .retain(|fundamental_subscription| *fundamental_subscription != subscription);
            }
            let mut strategy_subscriptions = self.strategy_subscriptions.write().await;
            if strategy_subscriptions.contains(&subscription) {
                strategy_subscriptions.retain(|x| x != &subscription);
            }
            *self.subscriptions_updated.write().await = true;
            return Ok(());
        }
        let mut handler = self.symbol_subscriptions.write().await;
        let symbol_handler = handler.get_mut(&subscription.symbol).unwrap();
        symbol_handler.unsubscribe(&subscription).await;
        if symbol_handler.active_count == 0 {
            handler.remove(&subscription.symbol);
        }
        let mut strategy_subscriptions = self.strategy_subscriptions.write().await;
        if strategy_subscriptions.contains(&subscription) {
            strategy_subscriptions.retain(|x| x != &subscription);
        }
        *self.subscriptions_updated.write().await = true;
        Ok(())
    }

    pub async fn subscriptions_updated(&self) -> bool {
        self.subscriptions_updated.read().await.clone()
    }

    pub async fn set_subscriptions_updated(&self, updated: bool) {
        *self.subscriptions_updated.write().await = updated;
    }

    /// Returns all the primary subscriptions
    /// These are subscriptions that come directly from the vendors own data source.
    /// They are not consolidators, but are the primary source of data for the consolidators.
    pub async fn primary_subscriptions(&self) -> Vec<DataSubscription> {
        let mut primary_subscriptions = vec![];
        for symbol_handler in self.symbol_subscriptions.read().await.values() {
            primary_subscriptions.push(symbol_handler.primary_subscription().await);
        }
        primary_subscriptions
    }

    /// Returns all the subscriptions including primary and consolidators
    pub async fn subscriptions(&self) -> Vec<DataSubscription> {
        let mut all_subscriptions = vec![];
        for symbol_handler in self.symbol_subscriptions.read().await.values() {
            all_subscriptions.append(&mut symbol_handler.all_subscriptions().await);
        }
        for subscription in self.fundamental_subscriptions.read().await.iter() {
            all_subscriptions.push(subscription.clone());
        }
        all_subscriptions
    }

    /// Updates any consolidators with primary data
    pub async fn update_time_slice(&self, time_slice: &TimeSlice) -> Option<TimeSlice> {
        let mut tasks = vec![];
        for base_data in time_slice.clone() {
            let symbol_subscriptions = self.symbol_subscriptions.clone();
            let task = tokio::spawn(async move {
                let base_data = base_data.clone();
                let symbol = base_data.symbol();
                let mut symbol_subscriptions = symbol_subscriptions.write().await;
                if let Some(symbol_handler) = symbol_subscriptions.get_mut(&symbol) {
                    symbol_handler.update(&base_data).await
                } else {
                    vec![]
                }
            });

            tasks.push(task);
        }

        // Await all tasks and collect the results
        let results: Vec<Vec<BaseDataEnum>> = join_all(tasks)
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

        match results.is_empty() {
            true => None,
            false => Some(results.into_iter().flatten().collect()),
        }
    }

    pub async fn update_consolidators_time(&self, time: DateTime<Utc>) -> Option<TimeSlice> {
        let mut symbol_subscriptions = self.symbol_subscriptions.write().await;
        let mut time_slice = TimeSlice::new();

        for (_, symbol_handler) in symbol_subscriptions.iter_mut() {
            match symbol_handler.update_time(time.clone()).await {
                Some(data) => {
                    time_slice.extend(data);
                }
                None => {}
            }
        }
        match time_slice.is_empty() {
            true => None,
            _ => Some(time_slice),
        }
    }

    pub async fn history(
        &self,
        subscription: &DataSubscription,
    ) -> Option<RollingWindow<BaseDataEnum>> {
        if subscription.base_data_type == BaseDataType::Fundamentals {
            return None;
        }
        if let Some(symbol_subscription) = self
            .symbol_subscriptions
            .read()
            .await
            .get(&subscription.symbol)
        {
            if let Some(consolidator) = symbol_subscription
                .secondary_subscriptions
                .get(subscription)
            {
                return Some(consolidator.history());
            }
        }
        None
    }

    pub async fn bar_index(
        &self,
        subscription: &DataSubscription,
        index: u64,
    ) -> Option<BaseDataEnum> {
        if subscription.base_data_type == BaseDataType::Fundamentals {
            return None;
        }
        if let Some(symbol_subscription) = self
            .symbol_subscriptions
            .read()
            .await
            .get(&subscription.symbol)
        {
            if let Some(consolidator) = symbol_subscription
                .secondary_subscriptions
                .get(subscription)
            {
                if consolidator.subscription() == subscription {
                    return consolidator.index(index);
                }
            }
        }
        None
    }

    pub async fn bar_current(&self, subscription: &DataSubscription) -> Option<BaseDataEnum> {
        if subscription.base_data_type == BaseDataType::Fundamentals {
            return None;
        }

        let symbol_subscriptions = self.symbol_subscriptions.read().await;
        if let Some(symbol_subscription) = symbol_subscriptions.get(&subscription.symbol) {
            let primary_subscription = symbol_subscription.primary_subscription().await;
            if &primary_subscription == subscription {
                return None;
            }
            if let Some(consolidator) = symbol_subscription
                .secondary_subscriptions
                .get(subscription)
            {
                if consolidator.subscription() == subscription {
                    return consolidator.current();
                }
            }
        }
        None
    }
}

/// This Struct Handles when to consolidate data for a subscription from an existing subscription.
/// Alternatively if a subscription is of a lower resolution subscription, then the new subscription becomes the primary data source and the existing subscription becomes the secondary data source.
/// depending if the vendor has data available in that resolution.
pub struct SymbolSubscriptionHandler {
    /// The primary subscription is the subscription where data is coming directly from the `DataVendor`, In the event of bar data, it is pre-consolidated.
    primary_subscription: DataSubscription,
    /// The secondary subscriptions are consolidators that are used to consolidate data from the primary subscription.
    secondary_subscriptions: AHashMap<DataSubscription, ConsolidatorEnum>,
    /// Count the subscriptions so we can delete the object if it is no longer being used
    active_count: i32,
    symbol: Symbol,
    subscription_event_buffer: Vec<DataSubscriptionEvent>,
    is_warmed_up: bool,
    primary_history: RollingWindow<BaseDataEnum>,
}

impl SymbolSubscriptionHandler {
    pub async fn new(
        primary_subscription: DataSubscription,
        is_warmed_up: bool,
        history_to_retain: u64,
        warm_up_to: DateTime<Utc>,
        strategy_mode: StrategyMode,
    ) -> Self {
        let mut handler = SymbolSubscriptionHandler {
            primary_subscription: primary_subscription.clone(),
            secondary_subscriptions: Default::default(),
            active_count: 1,
            symbol: primary_subscription.symbol.clone(),
            subscription_event_buffer: Default::default(),
            is_warmed_up,
            primary_history: RollingWindow::new(history_to_retain),
        };
        handler
            .select_primary_non_tick_subscription(
                primary_subscription,
                history_to_retain,
                warm_up_to,
                strategy_mode,
            )
            .await;
        handler
    }

    pub async fn update(&mut self, base_data: &BaseDataEnum) -> Vec<BaseDataEnum> {
        // Ensure we only process if the symbol matches
        if &self.symbol != base_data.symbol() {
            panic!(
                "Symbol mismatch: {:?} != {:?}",
                self.symbol,
                base_data.symbol()
            );
        }
        self.primary_history.add(base_data.clone());
        let mut consolidated_data = vec![];

        // Read the secondary subscriptions

        if self.secondary_subscriptions.is_empty() {
            return vec![];
        }

        // Iterate over the secondary subscriptions and update them
        for (_, consolidator) in self.secondary_subscriptions.iter_mut() {
            let data = consolidator.update(&base_data);
            consolidated_data.extend(data);
        }
        consolidated_data
    }

    pub async fn update_time(&mut self, time: DateTime<Utc>) -> Option<Vec<BaseDataEnum>> {
        let mut consolidated_data = vec![];
        // Iterate over the secondary subscriptions and update them
        for (_, consolidator) in self.secondary_subscriptions.iter_mut() {
            let data = consolidator.update_time(time.clone());
            consolidated_data.extend(data);
        }
        match consolidated_data.is_empty() {
            true => None,
            false => Some(consolidated_data),
        }
    }

    pub fn set_warmed_up(&mut self) {
        self.is_warmed_up = true;
    }

    pub fn get_subscription_event_buffer(&mut self) -> Vec<DataSubscriptionEvent> {
        let buffer = self.subscription_event_buffer.clone();
        self.subscription_event_buffer.clear();
        buffer
    }

    /// This is only used
    async fn select_primary_non_tick_subscription(
        &mut self,
        new_subscription: DataSubscription,
        history_to_retain: u64,
        to_time: DateTime<Utc>,
        strategy_mode: StrategyMode,
    ) {
        let available_resolutions: Vec<SubscriptionResolutionType> = new_subscription
            .symbol
            .data_vendor
            .resolutions(new_subscription.market_type.clone())
            .await
            .unwrap();
        //println!("Available Resolutions: {:?}", available_resolutions);
        if available_resolutions.is_empty() {
            panic!(
                "{} does not have any resolutions available",
                new_subscription.symbol.data_vendor
            );
        }
        let resolution_types = subscriptions::filter_resolutions(
            available_resolutions,
            new_subscription.resolution.clone(),
        );
        if resolution_types.is_empty() {
            panic!("{} does not have any resolutions available for {:?}, Problem in select_primary_non_tick_subscription or vendor.resolutions() fn", new_subscription.symbol.data_vendor, new_subscription);
        }

        let mut subscription_set = false;
        //if we have the resolution avaialable from the vendor, just use it.
        for subscription_resolution_type in &resolution_types {
            if subscription_resolution_type.resolution == new_subscription.resolution
                && subscription_resolution_type.base_data_type == new_subscription.base_data_type
            {
                self.subscription_event_buffer
                    .push(DataSubscriptionEvent::Subscribed(new_subscription.clone()));
                self.primary_subscription = new_subscription.clone();
                self.primary_history.clear();
                subscription_set = true;
                break;
            }
        }
        if !subscription_set {
            self.secondary_subscriptions.insert(
                new_subscription.clone(),
                ConsolidatorEnum::create_consolidator(
                    self.is_warmed_up,
                    new_subscription.clone(),
                    history_to_retain,
                    to_time,
                    strategy_mode,
                )
                .await,
            );

            self.active_count += 1;

            let max_resolution = resolution_types.iter().max_by_key(|r| r.resolution);
            if let Some(resolution_type) = max_resolution {
                let subscription = DataSubscription::new(
                    new_subscription.symbol.name.clone(),
                    new_subscription.symbol.data_vendor.clone(),
                    resolution_type.resolution,
                    resolution_type.base_data_type,
                    new_subscription.market_type.clone(),
                );

                self.primary_subscription = subscription
            }
        }
        self.active_count += 1;
    }

    async fn subscribe(
        &mut self,
        new_subscription: DataSubscription,
        history_to_retain: u64,
        to_time: DateTime<Utc>,
        strategy_mode: StrategyMode,
    ) {
        if self.all_subscriptions().await.contains(&new_subscription) {
            return;
        }
        match new_subscription.resolution {
            Resolution::Ticks(number) => {
                let res_type_tick =
                    SubscriptionResolutionType::new(Resolution::Ticks(1), BaseDataType::Ticks);
                if !new_subscription
                    .symbol
                    .data_vendor
                    .resolutions(new_subscription.market_type.clone())
                    .await
                    .unwrap()
                    .contains(&res_type_tick)
                {
                    panic!(
                        "{} does not have tick data available",
                        new_subscription.symbol.data_vendor
                    );
                }
                // we switch to tick data as base resolution for any tick subscription
                if number > 1 {
                    let consolidator = ConsolidatorEnum::create_consolidator(
                        self.is_warmed_up,
                        new_subscription.clone(),
                        history_to_retain,
                        to_time,
                        strategy_mode,
                    )
                    .await;
                    self.subscription_event_buffer
                        .push(DataSubscriptionEvent::Subscribed(
                            consolidator.subscription().clone(),
                        ));
                    self.secondary_subscriptions
                        .insert(consolidator.subscription().clone(), consolidator);
                }

                if self.primary_subscription.resolution != Resolution::Ticks(1) {
                    let new_primary_subscription = DataSubscription::new(
                        new_subscription.symbol.name.clone(),
                        new_subscription.symbol.data_vendor.clone(),
                        Resolution::Ticks(1),
                        new_subscription.base_data_type.clone(),
                        new_subscription.market_type.clone(),
                    );
                    self.primary_subscription = new_primary_subscription.clone();
                    self.subscription_event_buffer
                        .push(DataSubscriptionEvent::Subscribed(
                            new_primary_subscription.clone(),
                        ));
                }
            }
            _ => {
                // if the new subscription is of a lower resolution
                if new_subscription.resolution < self.primary_subscription.resolution {
                    self.select_primary_non_tick_subscription(
                        new_subscription,
                        history_to_retain,
                        to_time,
                        strategy_mode,
                    )
                    .await;
                } else {
                    //if we have no problem with adding new the resolution we can just add the new subscription as a consolidator
                    let consolidator = ConsolidatorEnum::create_consolidator(
                        self.is_warmed_up,
                        new_subscription.clone(),
                        history_to_retain,
                        to_time,
                        strategy_mode,
                    )
                    .await;
                    self.secondary_subscriptions
                        .insert(new_subscription.clone(), consolidator);
                    self.subscription_event_buffer
                        .push(DataSubscriptionEvent::Subscribed(new_subscription.clone()));
                }
            }
        }
        self.active_count += 1;
    }

    async fn unsubscribe(&mut self, subscription: &DataSubscription) {
        if subscription == &self.primary_subscription {
            if self.secondary_subscriptions.is_empty() {
                self.subscription_event_buffer
                    .push(DataSubscriptionEvent::Unsubscribed(subscription.clone()));
                self.active_count -= 1;
                return;
            }
        } else {
            //if subscription is not the primary subscription, then it must be a consolidator and can be removed without changing the primary subscription
            self.secondary_subscriptions.remove(subscription);
            self.subscription_event_buffer
                .push(DataSubscriptionEvent::Unsubscribed(subscription.clone()));
            self.active_count -= 1;
        }
    }

    pub async fn all_subscriptions(&self) -> Vec<DataSubscription> {
        let mut all_subscriptions = vec![self.primary_subscription.clone()];
        for (_, consolidator) in self.secondary_subscriptions.iter() {
            all_subscriptions.push(consolidator.subscription().clone());
        }
        all_subscriptions
    }

    pub async fn primary_subscription(&self) -> DataSubscription {
        self.primary_subscription.clone()
    }
}
