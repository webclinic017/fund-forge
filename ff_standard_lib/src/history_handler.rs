use std::collections::{VecDeque};
use std::sync::Mutex;
use ahash::AHashMap;
use crate::standardized_types::base_data::base_data_enum::BaseDataEnum;
use crate::standardized_types::subscriptions::DataSubscription;
use crate::standardized_types::time_slices::TimeSlice;

#[derive(Clone)]
pub struct RollingWindow {
    last: VecDeque<BaseDataEnum>,
    number: usize,
}

impl RollingWindow {
    pub fn new(number: usize) -> Self {
        RollingWindow {
            last: VecDeque::with_capacity(number),
            number,
        }
    }

    pub fn add(&mut self, data: BaseDataEnum) {
        if self.last.len() == self.number {
            self.last.pop_back(); // Remove the oldest data
        }
        self.last.push_front(data); // Add the latest data at the front
    }

    pub fn last(&self) -> Option<&BaseDataEnum> {
        self.last.front()
    }

    pub fn get(&self, index: usize) -> Option<&BaseDataEnum> {
        self.last.get(index)
    }

    pub fn len(&self) -> usize {
        self.last.len()
    }

    pub fn is_full(&self) -> bool {
        self.last.len() == self.number
    }

    pub fn clear(&mut self) {
        self.last.clear();
    }
}

pub struct HistoryHandler {
    history: Mutex<AHashMap<DataSubscription, RollingWindow>>,
    retain_last: usize,
}

impl HistoryHandler {
    pub fn new(retain_last: usize) -> Self {
        Self {
            history: Mutex::new(AHashMap::new()),
            retain_last,
        }
    }
    
  pub fn add(&self, time_slice: TimeSlice) {
    let mut history = self.history.lock().unwrap();
    for data in time_slice {
        let subscription = data.subscription();
        if !history.contains_key(&subscription) {
            history.insert(subscription.clone(), RollingWindow::new(self.retain_last));
        }
        let rolling_window = history.get_mut(&subscription).unwrap();
        rolling_window.add(data.clone());
    }
  }

    pub fn window(&self, subscription: &DataSubscription) -> RollingWindow {
        let mut history = self.history.lock().unwrap();
        if !history.contains_key(subscription) {
            history.insert(subscription.clone(), RollingWindow::new(self.retain_last));
        }
        history.get(subscription).unwrap().clone()
    }
    
    pub fn index(&self, subscription: &DataSubscription, index: usize) -> Option<BaseDataEnum> {
        let mut history = self.history.lock().unwrap();
        history.get(subscription).unwrap().get(index).cloned()
    }
    
    pub fn last(&self, subscription: &DataSubscription) -> BaseDataEnum {
        let mut history = self.history.lock().unwrap();
        history.get(subscription).unwrap().last().unwrap().clone()
    }
}
/*

pub async fn warmup(&mut self, current_time: DateTime<Utc>, strategy_mode: StrategyMode, time_zone: Tz) {
    match strategy_mode {
        StrategyMode::Backtest => {
            //let historical_duration =
            let vendor_resolutions = self.subscription.symbol.data_vendor.resolutions(self.subscription.market_type.clone()).await.unwrap();
            let mut minimum_resolution: Option<Resolution> = None;
            for resolution in vendor_resolutions {
                if minimum_resolution.is_none() {
                    minimum_resolution = Some(resolution);
                } else {
                    if resolution > minimum_resolution.unwrap() && resolution < self.subscription.resolution {
                        minimum_resolution = Some(resolution);
                    }
                }
            }

            let minimum_resolution = match minimum_resolution.is_none() {
                true => panic!("{} does not have any resolutions available", self.subscription.symbol.data_vendor),
                false => minimum_resolution.unwrap()
            };

            let data_type = match minimum_resolution {
                Resolution::Ticks(_) => BaseDataType::Ticks,
                _ => self.subscription.base_data_type.clone()
            };

            let start_date: NaiveDateTime = (current_time - (self.subscription.resolution.as_duration() * self.history.number as i32) - Duration::days(4)).naive_utc();
            let from_time = time_convert_utc_naive_to_fixed_offset(&time_zone, start_date);
            let to_time = time_convert_utc_naive_to_fixed_offset(&time_zone, current_time.naive_utc());

            let base_subscription = DataSubscription::new(self.subscription.symbol.name.clone(), self.subscription.symbol.data_vendor.clone(), minimum_resolution, data_type, self.subscription.market_type.clone());
            let base_data = range_data(from_time.to_utc(), to_time.to_utc(), base_subscription.clone()).await;

            for (time, slice) in &base_data {
                for base_data in slice {
                    if time <= &current_time {
                        self.update(base_data);
                    }
                }
            }
        },
        _ => {
            todo!() //get history from vendor so that it is most recent bars which may not be downloaded yet
        }
    }
}*/