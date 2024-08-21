use std::collections::{BTreeMap};
use std::sync::{Arc};
use std::time::Duration as StdDuration;
use chrono::{DateTime, TimeZone, Utc};
use futures::executor::block_on;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio::task;
use ff_standard_lib::standardized_types::base_data::base_data_enum::BaseDataEnum;
use ff_standard_lib::standardized_types::base_data::history::{generate_file_dates, get_historical_data};
use ff_standard_lib::standardized_types::base_data::traits::BaseData;
use ff_standard_lib::standardized_types::enums::{StrategyMode};
use ff_standard_lib::standardized_types::OwnerId;
use ff_standard_lib::standardized_types::time_slices::TimeSlice;
use ff_standard_lib::subscription_handler::SubscriptionHandler;
use crate::fund_forge_strategy::{FundForgeStrategy};
use crate::interaction_handler::InteractionHandler;
use crate::market_handlers::MarketHandlerEnum;
use crate::messages::strategy_events::{EventTimeSlice, StrategyEvent};
use crate::strategy_state::StrategyStartState;
//Possibly more accurate engine 
/*todo Use this for saving and loading data, it will make smaller file sizes and be less handling for consolidator, we can then just update historical data once per week on sunday and load last week from broker.   
   use Create a date (you can use DateTime<Utc>, Local, or NaiveDate)
   let date = Utc.ymd(2023, 8, 19); // Year, Month, Date
   let iso_week = date.iso_week();
   let week_number = iso_week.week(); // Get the week number (1-53)
   let week_year = iso_week.year(); // Get the year of the week (ISO week year)
   println!("The date {:?} is in ISO week {} of the year {}", date, week_number, week_year);
   The date 2023-08-19 is in ISO week 33 of the year 2023
 */

pub struct Engine {
    owner_id: OwnerId, 
    start_state: StrategyStartState, 
    strategy_event_sender: Sender<EventTimeSlice>, 
    subscription_handler: Arc<RwLock<SubscriptionHandler>>, 
    market_event_handler: Arc<RwLock<MarketHandlerEnum>>, 
    interaction_handler: Arc<RwLock<InteractionHandler>>
}

// The date 2023-08-19 is in ISO week 33 of the year 2023
impl Engine {
    pub fn new(owner_id: OwnerId, 
               start_state: StrategyStartState, 
               strategy_event_sender: Sender<EventTimeSlice>, 
               subscription_handler: Arc<RwLock<SubscriptionHandler>>, 
               market_event_handler: Arc<RwLock<MarketHandlerEnum>>, 
               interaction_handler: Arc<RwLock<InteractionHandler>>) -> Self {
        Engine {
            owner_id,
            start_state,
            strategy_event_sender,
            subscription_handler,
            market_event_handler,
            interaction_handler
        }
    }
    /// Initializes the strategy, runs the warmup and then runs the strategy based on the mode.
    /// Calling this method will start the strategy running.
    pub async fn launch(self: Self) {
        task::spawn(async move {
            println!("Initializing the strategy...");

            self.warmup().await;

            println!("Start Mode: {:?} ", self.start_state.mode);
            let msg = match self.start_state.mode {
                StrategyMode::Backtest => {
                    self.run_backtest().await;
                    "Backtest complete".to_string()
                }
                // All other modes use the live engine, just with different fns from the engine
                _ => {
                    self.run_live().await;
                    "Live complete".to_string()
                }
            };

            println!("{:?}", &msg);
            let end_event = StrategyEvent::ShutdownEvent(self.owner_id.clone(), msg);
            //todo() need a receiver for otehr event types
            match self.strategy_event_sender.send(vec![end_event]).await {
                Ok(_) => {},
                Err(e) => {
                    println!("Error forwarding event: {:?}", e);
                }
            }
        });
    }

    pub async fn run_live(&self) {
        todo!("Implement live mode");
        //need implementation for live plus live paper, where accounts are simulated locally. also make a hybrid mode so it trades live plus paper to test the engine functions accurately
    }


    /// Warm up runs on initialization to get the strategy ready for execution, this can be used to warm up indicators etc., to ensure the strategy has enough history to run from start to finish, without waiting for data.
    /// This is especially useful in live trading scenarios, else you could be waiting days for the strategy to warm up if you are dependent on a long history for your indicators etc. \
    /// \
    /// An alternative to the warmup, would be using the `history` method to get historical data for a specific subscription, and then manually warm up the indicators during re-balancing or adding new subscriptions.
    /// You don't need to be subscribed to an instrument to get the history, so that method is a good alternative for strategies that dynamically subscribe to instruments.
    async fn warmup(&self) {
        println!("Warming up the strategy...");
        let start_time = match self.start_state.mode {
            StrategyMode::Backtest => {
                self.start_state.start_date.to_utc()
            },
            //If Live we return the current time in utc time
            _ => Utc::now(),
        };

        // we run the historical data feed from the start time minus the warmup duration until we reach the start date for the strategy
        let warmup_start_time = start_time - self.start_state.warmup_duration;
        let month_years = generate_file_dates(warmup_start_time, start_time.clone());
        self.historical_data_feed(warmup_start_time, month_years, start_time.clone()).await;
        
        self.market_event_handler.write().await.set_warm_up_complete().await;
        let warmup_complete_event = StrategyEvent::WarmUpComplete(self.owner_id.clone());
        match self.strategy_event_sender.send(vec![warmup_complete_event]).await {
            Ok(_) => {},
            Err(e) => {
                println!("Error forwarding event: {:?}", e);
            }
        }
    }

    /// Runs the strategy backtest
    async fn run_backtest(&self) {
        println!("Running the strategy backtest...");
        {
            self.interaction_handler.read().await.process_controls().await;
        }

        // we run the historical data feed from the start time until we reach the end date for the strategy
        let start_time = self.start_state.start_date.clone();
        let end_date = self.start_state.end_date.clone();
        let month_years = generate_file_dates(start_time, end_date.clone());

        //println!("file dates: {:?}", month_years);

        self.historical_data_feed(start_time, month_years, end_date).await;

        self.market_event_handler.read().await.process_ledgers().await;

        // If we have reached the end of the backtest, we check that the last time recorded is not in the future, if it is, we set it to the current time.
    }


    /// Used by historical data feed to determine how to proceed with the historical data feed.
    /// Feeds the historical data to the strategy fixing
    async fn historical_data_feed(&self, mut last_time: DateTime<Utc>, month_years: BTreeMap<i32, DateTime<Utc>>, end_time: DateTime<Utc>) {
        // here we are looping through 1 month at a time, if the strategy updates its subscriptions we will stop the data feed, download the historical data again to include updated symbols, and resume from the next time to be processed.
        
        'main_loop: for (_, month_start) in month_years {
            'month_loop: loop {
                let subscription_handler = self.subscription_handler.read().await;
                subscription_handler.set_subscriptions_updated(false).await;
                let subscriptions = subscription_handler.primary_subscriptions().await;
                //println!("Month Loop Subscriptions: {:?}", subscriptions);
                let time_slices = match get_historical_data(subscriptions.clone(), month_start).await {
                    Ok(time_slices) => {
                        time_slices
                    },
                    Err(e) => {
                        println!("Error getting historical data for: {:?}. Error: {:?}", month_start, e);
                        break 'month_loop
                    }
                };

               let mut combined_data: BTreeMap<DateTime<Utc>, TimeSlice> = BTreeMap::new();
                'consolidator_loop: for (time, time_slice) in &time_slices {
                    if time < &last_time {
                        continue;
                    }
                    if time > &end_time {
                        break 'consolidator_loop
                    }
                    if !combined_data.contains_key(time) {
                        combined_data.insert(time.clone(), time_slice.clone());
                    } else {
                        let existing_time_slice = combined_data.get_mut(time).unwrap();
                        existing_time_slice.extend(time_slice.clone());
                    }

                    let consolidated_data = subscription_handler.update_consolidators(&time_slice).await;
                    if consolidated_data.len() == 0 {
                        continue;
                    }
                    for data in consolidated_data {
                        let (consolidated_time, consolidated_data_enum) = match &data {
                            BaseDataEnum::Candle(candle) => {
                                if candle.is_closed {
                                    // Use the candle's close time for closed candles
                                    (data.time_created_utc(), data)
                                } else {
                                    (time.clone(), data)
                                }
                            }
                            BaseDataEnum::QuoteBar(bar) => {
                                if bar.is_closed {
                                    // Use the candle's close time for closed candles
                                    (data.time_created_utc(), data)
                                } else {
                                    (time.clone(), data)
                                }
                            }
                            _ => panic!("Unsupported consolidated data type: {:?}", data)
                        };
                        if !combined_data.contains_key(&consolidated_time) {
                            combined_data.insert(consolidated_time, vec![consolidated_data_enum.clone()]);
                        } else {
                            combined_data.get_mut(&consolidated_time).unwrap().push(consolidated_data_enum.clone());
                        }
                    }
                }

                if combined_data.len() == 0 {
                    println!("No data found for: {:?}", month_start);
                    break 'month_loop
                }
                
                'slice_loop: for (time, time_slice) in &combined_data {
                    if time > &end_time {
                        break 'main_loop
                    }

                    if subscription_handler.subscriptions_updated().await {
                        last_time = time.clone();
                        break 'slice_loop
                    }

                    if time < &last_time {
                        continue;
                    }
                    
                    let mut strategy_event : EventTimeSlice = vec![];
                    strategy_event.push(StrategyEvent::TimeSlice(self.owner_id.clone(), time_slice.clone()));
                        // The market event handler response with any order events etc that may have been returned from the base data update
                    match time_slices.get(time) {
                        None => {}
                        Some(base_slice) => {
                            let market_event_handler_events = self.market_event_handler.write().await.base_data_upate(base_slice.clone()).await;
                            match market_event_handler_events {
                                Some(events) => {
                                    strategy_event.extend(events);
                                },
                                None => {},
                            }
                        }
                    }

                    {
                        self.market_event_handler.read().await.update_time(time.clone()).await;
                    }
                    match self.strategy_event_sender.send(strategy_event).await {
                        Ok(_) => {},
                        Err(e) => {
                            println!("Error forwarding event: {:?}", e);
                        }
                    }
                    
                    // Update the last processed time
                    last_time = time.clone();
                    

                    // We check if the user has requested a delay between time slices for market replay style backtesting.
                    match self.interaction_handler.read().await.replay_delay_ms().await {
                        Some(delay) => tokio::time::sleep(StdDuration::from_millis(delay)).await,
                        None => {},
                    }

                    // We check if the user has input any strategy commands, pause, play, stop etc.
                    if self.interaction_handler.read().await.process_controls().await == true {
                        break 'main_loop
                    }
                }
            };
        }
    }
}