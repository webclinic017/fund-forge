use chrono::{DateTime, Utc};
use crate::apis::vendor::client_requests::ClientSideDataVendor;
use crate::standardized_types::rolling_window::RollingWindow;
use crate::standardized_types::base_data::base_data_enum::BaseDataEnum;
use crate::standardized_types::base_data::base_data_type::BaseDataType;
use crate::standardized_types::base_data::candle::Candle;
use crate::standardized_types::base_data::quotebar::QuoteBar;
use crate::standardized_types::base_data::traits::BaseData;
use crate::standardized_types::enums::{Resolution};
use crate::standardized_types::subscriptions::{CandleType, DataSubscription};
use crate::consolidators::count::ConsolidatorError;
use crate::helpers::converters;
use crate::helpers::decimal_calculators::round_to_tick_size;

pub struct CandleStickConsolidator {
    current_data: Option<BaseDataEnum>,
    pub(crate) subscription: DataSubscription,
    pub(crate) history: RollingWindow<BaseDataEnum>,
    tick_size: f64,
}

impl CandleStickConsolidator {
    pub fn update_time(&mut self, time: DateTime<Utc>) -> Vec<BaseDataEnum> {
        if let Some(current_data) = self.current_data.as_mut() {
            if time >= current_data.time_created_utc() {
                let return_data = current_data.clone();
                self.current_data = None;
                return vec![return_data];
            }
        }
        vec![]
    }
    
    fn update_candles(&mut self, base_data: &BaseDataEnum) -> Vec<BaseDataEnum> {
        if self.current_data.is_none() {
            let data = self.new_candle(base_data);
            self.current_data = Some(BaseDataEnum::Candle(data));
            let candles = vec![self.current_data.clone().unwrap()];
            return candles
        } else if let Some(current_bar) = self.current_data.as_mut() {
            if base_data.time_created_utc() >= current_bar.time_created_utc() {
                let mut consolidated_bar = current_bar.clone();
                consolidated_bar.set_is_closed(true);
                self.history.add(consolidated_bar.clone());

                let new_bar = self.new_candle(base_data);
                self.current_data = Some(BaseDataEnum::Candle(new_bar.clone()));
                return vec![consolidated_bar, BaseDataEnum::Candle(new_bar)]
            }
            match current_bar {
                BaseDataEnum::Candle(candle) => {
                    match base_data {
                        BaseDataEnum::Tick(tick) => {
                            candle.high = candle.high.max(tick.price);
                            candle.low = candle.low.min(tick.price);
                            candle.close = tick.price;
                            candle.range = round_to_tick_size(candle.high - candle.low, self.tick_size);
                            candle.volume += tick.volume;
                            return vec![BaseDataEnum::Candle(candle.clone())]
                        },
                        BaseDataEnum::Candle(new_candle) => {
                            candle.high = candle.high.max(new_candle.high);
                            candle.low = candle.low.min(new_candle.low);
                            candle.range = round_to_tick_size(candle.high - candle.low, self.tick_size);
                            candle.close = new_candle.close;
                            candle.volume += new_candle.volume;
                            return vec![BaseDataEnum::Candle(candle.clone())]
                        },
                        BaseDataEnum::Price(price) => {
                            candle.high = candle.high.max(price.price);
                            candle.low = candle.low.min(price.price);
                            candle.range = round_to_tick_size(candle.high - candle.low, self.tick_size);
                            candle.close = price.price;
                            return vec![BaseDataEnum::Candle(candle.clone())]
                        },
                        _ => panic!("Invalid base data type for Candle consolidator: {}", base_data.base_data_type())
                    }
                },
                _ =>  panic!("Invalid base data type for Candle consolidator: {}", base_data.base_data_type())
            }
        }
        panic!("Invalid base data type for Candle consolidator: {}", base_data.base_data_type())
    }

    fn new_quote_bar(&self, new_data: &BaseDataEnum) -> QuoteBar {
        let time = converters::open_time(&self.subscription, new_data.time_utc());
        match new_data {
            BaseDataEnum::QuoteBar(bar) => {
                let mut new_bar = bar.clone();
                new_bar.is_closed = false;
                new_bar.time = time.to_string();
                new_bar.resolution = self.subscription.resolution.clone();
                new_bar
            },
            BaseDataEnum::Quote(quote) => QuoteBar::new(self.subscription.symbol.clone(), quote.bid, quote.ask, 0.0, time.to_string(), self.subscription.resolution.clone(), CandleType::CandleStick),
            _ => panic!("Invalid base data type for QuoteBar consolidator"),
        }
    }

    /// We can use if time == some multiple of resolution then we can consolidate, we dont need to know the actual algo time, because we can get time from the base_data if self.last_time >
    fn update_quote_bars(&mut self, base_data: &BaseDataEnum) -> Vec<BaseDataEnum> {
        if self.current_data.is_none() {
            let data = self.new_quote_bar(base_data);
            self.current_data = Some(BaseDataEnum::QuoteBar(data));
            return vec![self.current_data.clone().unwrap()]
        } else if let Some(current_bar) = self.current_data.as_mut() {
            if base_data.time_created_utc() >= current_bar.time_created_utc() {
                let mut consolidated_bar = current_bar.clone();
                consolidated_bar.set_is_closed(true);
                self.history.add(consolidated_bar.clone());
                let new_bar = self.new_quote_bar(base_data);
                self.current_data = Some(BaseDataEnum::QuoteBar(new_bar.clone()));
                return vec![consolidated_bar, BaseDataEnum::QuoteBar(new_bar)]
            }
            match current_bar {
                BaseDataEnum::QuoteBar(quote_bar) => {
                    match base_data {
                        BaseDataEnum::Quote(quote) => {
                            quote_bar.ask_high = quote_bar.ask_high.max(quote.ask);
                            quote_bar.ask_low = quote_bar.ask_low.min(quote.ask);
                            quote_bar.bid_high = quote_bar.bid_high.max(quote.bid);
                            quote_bar.bid_low = quote_bar.bid_low.min(quote.bid);
                            quote_bar.range = round_to_tick_size(quote_bar.ask_high - quote_bar.bid_low, self.tick_size);
                            quote_bar.ask_close = quote.ask;
                            quote_bar.bid_close = quote.bid;
                            return vec![BaseDataEnum::QuoteBar(quote_bar.clone())]
                        },
                        BaseDataEnum::QuoteBar(bar) => {
                            quote_bar.ask_high = quote_bar.ask_high.max(bar.ask_high);
                            quote_bar.ask_low = quote_bar.ask_low.min(bar.ask_low);
                            quote_bar.bid_high = quote_bar.bid_high.max(bar.bid_high);
                            quote_bar.bid_low = bar.bid_low.min(bar.bid_low);
                            quote_bar.range = round_to_tick_size(quote_bar.ask_high - quote_bar.bid_low, self.tick_size);
                            quote_bar.ask_close = bar.ask_close;
                            quote_bar.bid_close = bar.bid_close;
                            quote_bar.volume += bar.volume;
                            return vec![BaseDataEnum::QuoteBar(quote_bar.clone())]
                        },
                        _ =>  panic!("Invalid base data type for QuoteBar consolidator: {}", base_data.base_data_type())

                    }
                }
                _ =>  panic!("Invalid base data type for QuoteBar consolidator: {}", base_data.base_data_type())
            }
        }
        panic!("Invalid base data type for QuoteBar consolidator: {}", base_data.base_data_type())
    }

    fn new_candle(&self, new_data: &BaseDataEnum) -> Candle {
        let time = converters::open_time(&self.subscription, new_data.time_utc());
        match new_data {
            BaseDataEnum::Tick(tick) => Candle::new(self.subscription.symbol.clone(), tick.price, tick.volume, time.to_string(), self.subscription.resolution.clone(), self.subscription.candle_type.clone().unwrap()),
            BaseDataEnum::Candle(candle) => {
                let mut consolidated_candle = candle.clone();
                consolidated_candle.is_closed = false;
                consolidated_candle.resolution = self.subscription.resolution.clone();
                consolidated_candle.time = time.to_string();
                consolidated_candle
            },
            BaseDataEnum::Price(price) => Candle::new(self.subscription.symbol.clone(), price.price, 0.0, time.to_string(), self.subscription.resolution.clone(), self.subscription.candle_type.clone().unwrap()),
            _ => panic!("Invalid base data type for Candle consolidator")
        }
    }
    
    pub(crate) async fn new(subscription: DataSubscription, history_to_retain: u64) -> Result<Self, ConsolidatorError> {
        if subscription.base_data_type == BaseDataType::Fundamentals {
            return Err(ConsolidatorError { message: format!("{} is an Invalid base data type for TimeConsolidator", subscription.base_data_type) });
        }

        if let Resolution::Ticks(_) = subscription.resolution {
            return Err(ConsolidatorError { message: format!("{:?} is an Invalid resolution for TimeConsolidator", subscription.resolution) });
        }

        let tick_size = match subscription.symbol.data_vendor.tick_size(subscription.symbol.clone()).await {
            Ok(size) => size,
            Err(e) => return Err(ConsolidatorError { message: format!("Error getting tick size: {}", e) }),
        };
        
        Ok(CandleStickConsolidator {
            current_data: None,
            subscription,
            history: RollingWindow::new(history_to_retain),
            tick_size,
        })
    }
    
    pub(crate) fn update(&mut self, base_data: &BaseDataEnum) -> Vec<BaseDataEnum> {
        match base_data.base_data_type() {
            BaseDataType::Ticks => {
                if self.subscription.base_data_type == BaseDataType::Candles {
                    return self.update_candles(base_data);
                }
            },
            BaseDataType::Quotes => {
                if self.subscription.base_data_type == BaseDataType::QuoteBars {
                    return self.update_quote_bars(base_data);
                }
            },
            BaseDataType::Prices => {
                if self.subscription.base_data_type == BaseDataType::Candles {
                    return self.update_candles(base_data);
                }
            }
            BaseDataType::QuoteBars => {
                if self.subscription.base_data_type == BaseDataType::QuoteBars {
                    return self.update_quote_bars(base_data);
                }
            }
            BaseDataType::Candles => {
                if self.subscription.base_data_type == BaseDataType::Candles {
                    return self.update_candles(base_data);
                }
            }
            BaseDataType::Fundamentals => panic!("Fundamentals are not supported"),
        }
        vec![]
    }

    pub(crate) fn history(&self) -> RollingWindow<BaseDataEnum> {
        self.history.clone()
    }


    pub(crate) fn index(&self, index: u64) -> Option<BaseDataEnum> {
        match self.history.get(index) {
            Some(data) => Some(data.clone()),
            None => None,
        }
    }

    pub(crate) fn current(&self) -> Option<BaseDataEnum> {
        match &self.current_data {
            Some(data) => Some(data.clone()),
            None => None,
        }
    }
}


