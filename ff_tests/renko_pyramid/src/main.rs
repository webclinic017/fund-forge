use chrono::{Duration, NaiveDate, Utc};
use chrono_tz::Australia;
use colored::Colorize;
use ff_standard_lib::apis::rithmic::rithmic_systems::RithmicSystem;
use rust_decimal::Decimal;
use ff_standard_lib::standardized_types::base_data::traits::BaseData;
use ff_standard_lib::standardized_types::enums::{FuturesExchange, MarketType, OrderSide, PositionSide, StrategyMode};
use ff_standard_lib::strategies::strategy_events::{StrategyEvent};
use ff_standard_lib::standardized_types::subscriptions::{DataSubscription, SymbolCode, SymbolName};
use ff_standard_lib::strategies::fund_forge_strategy::FundForgeStrategy;
use rust_decimal_macros::dec;
use tokio::sync::mpsc;
use ff_standard_lib::gui_types::settings::Color;
use ff_standard_lib::standardized_types::accounts::{Account, Currency};
use ff_standard_lib::standardized_types::base_data::base_data_type::BaseDataType;
use ff_standard_lib::standardized_types::broker_enum::Brokerage;
use ff_standard_lib::standardized_types::datavendor_enum::DataVendor;
use ff_standard_lib::standardized_types::orders::{OrderUpdateEvent, TimeInForce};
use ff_standard_lib::standardized_types::position::PositionUpdateEvent;
use ff_standard_lib::standardized_types::resolution::Resolution;
use ff_standard_lib::strategies::indicators::built_in::renko::Renko;
use ff_standard_lib::strategies::indicators::indicator_enum::IndicatorEnum;
use ff_standard_lib::strategies::indicators::indicator_events::IndicatorEvents;

#[tokio::main]
async fn main() {
    let (strategy_event_sender, strategy_event_receiver) = mpsc::channel(100);
    let account = Account::new(Brokerage::Rithmic(RithmicSystem::Apex), "APEX-3396-168".to_string());
    let symbol_code = SymbolCode::from("MESZ4");
    let symbol_name = SymbolName::from("MES");
    let subscription = DataSubscription::new(
        symbol_name.clone(),
        DataVendor::Rithmic(RithmicSystem::Apex),
        Resolution::Ticks(1),
        BaseDataType::Ticks,
        MarketType::Futures(FuturesExchange::CME),
    );

    let strategy = FundForgeStrategy::initialize(
        StrategyMode::LivePaperTrading,
        dec!(100000),
        Currency::USD,
        NaiveDate::from_ymd_opt(2024, 6, 5).unwrap().and_hms_opt(0, 0, 0).unwrap(),
        NaiveDate::from_ymd_opt(2024, 6, 15).unwrap().and_hms_opt(0, 0, 0).unwrap(),
        Australia::Sydney,
        Duration::hours(1),
        vec![
            subscription.clone()
        ],
        false,
        100,
        strategy_event_sender,
        core::time::Duration::from_millis(5),
        false,
        false,
        true,
        vec![account.clone()],
    ).await;

    on_data_received(strategy, strategy_event_receiver, subscription, symbol_code, symbol_name, account).await;
}

// This strategy is designed to pyramid into strong trends using renko. It will not work trading mean reverting markets or trading in both directions.
// It is a tool to help manage positions in fast trending markets. In the current state fund forge strategies should not be run without monitoring. It is possible strategies can lose sync with the actual broker account state.
// 1. MOMO MODE: It enters 2 bullish bars. opposite for short. REVERSAL MODE: It if we have 2 bars but 3 bars ago was reversal against the trend (assuming we are with the trend).
// 2. It exits after 2 bearish renko bars. opposite for short.
// 3. It adds on repeat signals up to 4 times, only if it is in profit.
// 4. It takes profit after a certain amount of profit is made if it is at max size. It will do this with limit orders that expire in X seconds.
// 5. The limit order expiry is on the exchange/rithmic side.
// 6. It will cancel the take profit order if the position is closed.

const RENKO_RANGE: Decimal = dec!(2);
const MAX_SIZE: Decimal = dec!(20);
const SIZE: Decimal = dec!(5);
const INCREMENTAL_SCALP_PNL: Decimal = dec!(50);
const LIMIT_ORDER_EXPIRE_IN_SECS: i64 = 60;
const TRADING_LONG: bool = false;
const TRADING_SHORT: bool = true;
const MOMENTUM: bool = false; //if true we will enter on 2 blocks (2 bull blocks for bull entry), if false we will enter on reversal (1 bear block then 2 bull blocks)

#[allow(clippy::const_err)]
pub async fn on_data_received(
    strategy: FundForgeStrategy,
    mut event_receiver: mpsc::Receiver<StrategyEvent>,
    subscription: DataSubscription,
    symbol_code: SymbolCode,
    symbol_name: SymbolName,
    account: Account
) {
    println!("Starting Renko Pyramid Strategy with parameters: Renko Range: {}, Max Size: {}, Size: {}, Incremental Scalp PNL: {}, Limit Order Expire in Secs: {}, Trading Long: {}, Trading Short: {}, Is Momentum: {}", RENKO_RANGE, MAX_SIZE, SIZE, INCREMENTAL_SCALP_PNL, LIMIT_ORDER_EXPIRE_IN_SECS, TRADING_LONG, TRADING_SHORT, MOMENTUM);

    let renko = "renko".to_string();
    let renko_indicator = IndicatorEnum::Renko(Renko::new(renko.clone(), subscription.clone(), RENKO_RANGE, Color::new(0, 128, 0), Color::new(128, 0, 0), 20).await);
    strategy.subscribe_indicator(renko_indicator, false).await;
    let open = "open".to_string();
    let close = "close".to_string();
    let mut warmup_complete = false;
    let mut entry_order_id = None;
    let mut exit_order_id = None;
    let mut tp_id = None;
    let mut bars_since_entry = 0;
    let mut last_short_result = Result::BreakEven;
    let mut last_long_result = Result::BreakEven;
    // The engine will send a buffer of strategy events at the specified buffer interval, it will send an empty buffer if no events were buffered in the period.
    'strategy_loop: while let Some(strategy_event) = event_receiver.recv().await {
        //println!("Strategy: Buffer Received Time: {}", strategy.time_local());
        //println!("Strategy: Buffer Event Time: {}", strategy.time_zone().from_utc_datetime(&time.naive_utc()));
        match strategy_event {
            StrategyEvent::IndicatorEvent(event) => {
                match event {
                    IndicatorEvents::IndicatorTimeSlice(slice) => {
                        let mut no_entry = true;
                        let mut no_exit = true;
                        for renko_value in slice {
                            if let (Some(block_open), Some(block_close)) = (renko_value.get_plot(&open), renko_value.get_plot(&close)) {
                                let msg = format!("Renko: Open: {}, Close: {} @ {}", block_open.value, block_close.value, strategy.time_local());
                                if block_close.value > block_open.value {
                                    println!("{}", msg.as_str().bright_green());
                                } else if block_close.value < block_open.value {
                                    println!("{}", msg.as_str().bright_red());
                                }

                                if let (Some(last_block), Some(two_blocks_ago)) = (strategy.indicator_index(&renko, 1), strategy.indicator_index(&renko, 2)) {
                                    let last_close = last_block.get_plot(&close).unwrap().value;
                                    let last_open = last_block.get_plot(&open).unwrap().value;
                                    let two_blocks_ago_close = two_blocks_ago.get_plot(&close).unwrap().value;
                                    let two_blocks_ago_open = two_blocks_ago.get_plot(&open).unwrap().value;

                                    #[allow(clippy::const_err)]
                                    if TRADING_LONG {
                                        let is_long = strategy.is_long(&account, &symbol_code);

                                        if is_long {
                                            bars_since_entry += 1;
                                        }

                                        // Buy on 2 bullish renko blocks
                                        if block_close.value > block_open.value && last_close > last_open && (MOMENTUM || !MOMENTUM && two_blocks_ago_close < two_blocks_ago_open) && no_entry && entry_order_id == None
                                            && (is_long == false || strategy.pnl(&account, &symbol_code) > INCREMENTAL_SCALP_PNL / dec!(3)) {
                                            let quantity = strategy.position_size(&account, &symbol_code);
                                            if !strategy.is_long(&account, &symbol_code) && quantity < MAX_SIZE {
                                                entry_order_id = Some(strategy.enter_long(&symbol_name, None, &account, None, SIZE, String::from("Enter Long")).await);
                                                no_entry = false;
                                            }
                                        }
                                        if is_long {
                                            //tp on 2 bearish renko blocks
                                            if last_close < last_open && block_close.value < block_open.value && no_exit && exit_order_id == None {
                                                let quantity = strategy.position_size(&account, &symbol_code);
                                                exit_order_id = Some(strategy.exit_long(&symbol_name, None, &account, None, quantity, String::from("Exit Long")).await);
                                                no_exit = false;
                                            }

                                            let profit = strategy.pnl(&account, &symbol_code);
                                            let quantity = strategy.position_size(&account, &symbol_code);
                                            if profit > INCREMENTAL_SCALP_PNL && quantity == MAX_SIZE && exit_order_id == None && tp_id == None {
                                                let tif = TimeInForce::Time((Utc::now() + Duration::seconds(LIMIT_ORDER_EXPIRE_IN_SECS)).timestamp());
                                                tp_id = Some(strategy.limit_order(&symbol_name, None, &account, None, SIZE, OrderSide::Sell, last_close + RENKO_RANGE * dec!(4), tif, String::from("Partial TP Long")).await);
                                                no_exit = false;
                                            }
                                        }
                                    }
                                    #[allow(clippy::const_err)]
                                    if TRADING_SHORT {
                                        let is_short = strategy.is_short(&account, &symbol_code);

                                        if is_short {
                                            bars_since_entry += 1;
                                        }

                                        // Buy on 2 bearish renko blocks
                                        if block_close.value < block_open.value && last_close < last_open && (MOMENTUM || !MOMENTUM && two_blocks_ago_close > two_blocks_ago_open) && no_entry && entry_order_id == None
                                            && (is_short == false || strategy.pnl(&account, &symbol_code) > INCREMENTAL_SCALP_PNL / dec!(3)) {
                                            let quantity = strategy.position_size(&account, &symbol_code);
                                            if !strategy.is_long(&account, &symbol_code) && quantity < MAX_SIZE {
                                                entry_order_id = Some(strategy.enter_short(&symbol_name, None, &account, None, SIZE, String::from("Enter Short")).await);
                                                no_entry = false;
                                            }
                                        }
                                        if is_short {
                                            //tp on 2 bullish renko blocks
                                            if last_close > last_open && block_close.value > block_open.value && no_exit && exit_order_id == None {
                                                let quantity = strategy.position_size(&account, &symbol_code);
                                                exit_order_id = Some(strategy.exit_short(&symbol_name, None, &account, None, quantity, String::from("Exit Short")).await);
                                                no_exit = false;
                                            }

                                            let profit = strategy.pnl(&account, &symbol_code);
                                            let quantity = strategy.position_size(&account, &symbol_code);
                                            if profit > INCREMENTAL_SCALP_PNL && quantity == MAX_SIZE && exit_order_id == None && tp_id == None {
                                                let tif = TimeInForce::Time((Utc::now() + Duration::seconds(LIMIT_ORDER_EXPIRE_IN_SECS)).timestamp());
                                                tp_id = Some(strategy.limit_order(&symbol_name, None, &account, None, SIZE, OrderSide::Buy, last_close - RENKO_RANGE * dec!(4), tif, String::from("Partial TP Short")).await);
                                                no_exit = false;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        let pnl = strategy.pnl(&account, &symbol_code);
                        let quantity = strategy.position_size(&account, &symbol_code);
                        let msg = format!("{} Strategy: PNL: {}, Quantity: {}", symbol_code, pnl, quantity);
                        println!("{}", msg.as_str().bright_blue());
                    }
                    _ => {}
                }
            }
            StrategyEvent::TimeSlice(_) => {}
            StrategyEvent::ShutdownEvent(event) => {
                strategy.flatten_all_for(account).await;
                let msg = format!("{}",event);
                println!("{}", msg.as_str().bright_magenta());
                strategy.export_trades(&String::from("./trades exports"));
                strategy.print_ledgers().await;
                //we should handle shutdown gracefully by first ending the strategy loop.
                break 'strategy_loop
            },

            StrategyEvent::WarmUpComplete => {
                let msg = String::from("Strategy: Warmup Complete");
                println!("{}", msg.as_str().bright_magenta());
                warmup_complete = true;
            }

            StrategyEvent::PositionEvents(event) => {
                match event {
                    PositionUpdateEvent::PositionOpened { .. } => {}
                    PositionUpdateEvent::Increased { .. } => {}
                    PositionUpdateEvent::PositionReduced { .. } => {
                        strategy.print_ledger(event.account()).await;
                    },
                    PositionUpdateEvent::PositionClosed { ref side, ref booked_pnl,.. } => {
                        strategy.print_ledger(event.account()).await;
                        exit_order_id = None;
                        entry_order_id = None;
                        let result = if *booked_pnl > dec!(0) {
                            Result::Win
                        } else if *booked_pnl < dec!(0) {
                            Result::Loss
                        } else {
                            Result::BreakEven
                        };
                        match side {
                            PositionSide::Long => {
                               last_long_result = result;
                            }
                            PositionSide::Short => {
                                last_short_result = result;
                            }
                        }
                    },
                }
                let quantity = strategy.position_size(&account, &symbol_code);
                let msg = format!("{}, Time Local: {}", event, event.time_local(strategy.time_zone()));
                println!("{}", msg.as_str().purple());
                println!("Strategy: Open Quantity: {}", quantity);
            }
            StrategyEvent::OrderEvents(event) => {
                let msg = format!("Strategy: Order Event: {}, Time: {}", event, event.time_local(strategy.time_zone()));
                match event {
                    OrderUpdateEvent::OrderRejected { .. } => {
                        strategy.print_ledger(event.account()).await;
                        println!("{}", msg.as_str().on_bright_magenta().on_bright_red());
                        if let Some(order_id) = &entry_order_id {
                            if event.order_id() == order_id {
                                entry_order_id = None;
                            }
                        }
                        if let Some(order_id) = &exit_order_id {
                            if event.order_id() == order_id {
                                exit_order_id = None;
                            }
                        }
                        if let Some(order_id) = &tp_id {
                            if event.order_id() == order_id {
                                tp_id = None;
                            }
                        }
                    },
                    OrderUpdateEvent::OrderCancelled { .. }  => {
                        strategy.print_ledger(event.account()).await;
                        println!("{}", msg.as_str().on_bright_magenta().on_bright_yellow());
                        if let Some(order_id) = &entry_order_id {
                            if event.order_id() == order_id {
                                entry_order_id = None;
                            }
                        }
                        if let Some(order_id) = &exit_order_id {
                            if event.order_id() == order_id {
                                exit_order_id = None;
                            }
                        }
                        if let Some(order_id) = &tp_id {
                            if event.order_id() == order_id {
                                tp_id = None;
                            }
                        }
                    },
                    OrderUpdateEvent::OrderFilled {..} => {
                        strategy.print_ledger(event.account()).await;
                        println!("{}", msg.as_str().on_bright_magenta().on_bright_yellow());
                        if let Some(order_id) = &entry_order_id {
                            if event.order_id() == order_id {
                                entry_order_id = None;
                                bars_since_entry = 0;
                            }
                        }
                        if let Some(order_id) = &exit_order_id {
                            if event.order_id() == order_id {
                                exit_order_id = None;
                                if let Some(order_id) = &tp_id {
                                    strategy.cancel_order(order_id.clone()).await;
                                }
                                if !strategy.is_long(&account, &symbol_code) {
                                    bars_since_entry = 0;
                                }
                            }
                        }
                        if let Some(order_id) = &tp_id {
                            if event.order_id() == order_id {
                                tp_id = None;
                            }
                            if !strategy.is_long(&account, &symbol_code) {
                                bars_since_entry = 0;
                            }
                        }
                    },
                    _ =>  println!("{}", msg.as_str().bright_yellow())
                }
            }
            StrategyEvent::TimedEvent(name) => {
                println!("{} has triggered", name);
            }
            _ => {}
        }
    }
    event_receiver.close();
    println!("Strategy: Event Loop Ended");
}

enum Result {
    Win,
    Loss,
    BreakEven
}