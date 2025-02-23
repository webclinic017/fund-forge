use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::oneshot;
use crate::messages::data_server_messaging::{DataServerRequest, DataServerResponse, FundForgeError};
use crate::product_maps::oanda::maps::{get_oanda_symbol_names, OANDA_SYMBOL_INFO, SYMBOL_DIVISORS};
use crate::product_maps::rithmic::maps::{find_base_symbol, get_available_rithmic_symbol_names, get_rithmic_intraday_margin_in_usd, get_futures_symbol_info};
use crate::standardized_types::accounts::{AccountId, AccountInfo, Currency};
use crate::standardized_types::broker_enum::Brokerage;
use crate::standardized_types::new_types::{Price, Volume};
use crate::standardized_types::subscriptions::SymbolName;
use crate::standardized_types::symbol_info::{CommissionInfo, SymbolInfo};
use crate::strategies::client_features::connection_types::ConnectionType;
use crate::strategies::client_features::request_handler::{send_request, StrategyRequest};

impl Brokerage {
    pub async fn intraday_margin_required(&self, symbol_name: &SymbolName, quantity: Volume, price: Price, account_currency: Currency, base_currency: Option<Currency>, position_currency: Currency, conversion_rate: Decimal) -> Result<Option<Decimal>, FundForgeError> {
        match self {
            // Test broker uses simple leverage
            Brokerage::Test => {
                let base_margin = quantity * price; // Calculate margin in position currency
                Ok(Some(base_margin * conversion_rate / dec!(30))) // Convert to account currency and apply leverage
            },

            // Rithmic provides margins in USD
            Brokerage::Rithmic(_) => {
                match get_rithmic_intraday_margin_in_usd(symbol_name) {
                    Some(margin) => Ok(Some(margin * quantity * conversion_rate)),
                    None => Ok(None)
                }
            },

            Brokerage::Oanda => {
                match SYMBOL_DIVISORS.get(symbol_name.as_str()) {
                    Some(divisor) => {
                        let margin_value = if let Some(base_curr) = base_currency {
                            if account_currency == base_curr {
                                quantity  // Already in account currency
                            } else if account_currency == position_currency {
                                quantity * price  // Value is in quote currency already
                            } else {
                                // For EUR account trading AUD/JPY:
                                let position_value = quantity / price;  // Gets us 10,000,000 JPY
                                // Rate is JPY/EUR (0.00614), can multiply directly
                                position_value * conversion_rate  // Convert JPY to EUR
                            }
                        } else {
                            // Non-currency pair logic unchanged
                            quantity * price * conversion_rate
                        };

                        Ok(Some(margin_value / divisor))
                    },
                    None => Err(FundForgeError::ClientSideErrorDebug(format!("Symbol not found: {}", symbol_name)))
                }
            }

            // Bitget needs spot vs futures handling
            Brokerage::Bitget => {
                let base_margin = quantity * price; // Calculate margin in position currency
                Ok(Some(base_margin * conversion_rate)) // Convert to account currency
            }
        }
    }

    pub async fn symbol_info(&self, symbol_name: SymbolName) -> Result<SymbolInfo, FundForgeError> {
        match self {
            Brokerage::Rithmic(_) => {
                match get_futures_symbol_info(&symbol_name) {
                    Ok(symbol_info) => Ok(symbol_info),
                    Err(_) => {
                        match find_base_symbol(&symbol_name) {
                            None => return Err(FundForgeError::ClientSideErrorDebug(format!("Symbol info not found: {}", symbol_name))),
                            Some(symbol) => {
                                return match get_futures_symbol_info(&symbol) {
                                    Ok(info) => Ok(info),
                                    Err(e) => Err(FundForgeError::ClientSideErrorDebug(format!("{}", e)))
                                }
                            }
                        };
                    }
                }
            }
            Brokerage::Oanda => {
                match OANDA_SYMBOL_INFO.get(&symbol_name) {
                    Some(info) => Ok(info.clone()),
                    None => Err(FundForgeError::ClientSideErrorDebug(format!("Symbol info not found for symbol: {}", symbol_name)))
                }
            }
            _ => {
                let request = DataServerRequest::SymbolInfo {
                    callback_id: 0,
                    brokerage: self.clone(),
                    symbol_name,
                };
                let (sender, receiver) = oneshot::channel();
                let msg = StrategyRequest::CallBack(ConnectionType::Broker(self.clone()), request, sender);
                send_request(msg).await;
                match receiver.await {
                    Ok(response) => match response {
                        DataServerResponse::SymbolInfo { symbol_info, .. } => Ok(symbol_info),
                        DataServerResponse::Error { error, .. } => Err(error),
                        _ => Err(FundForgeError::ClientSideErrorDebug("Incorrect response received at callback".to_string()))
                    },
                    Err(e) => Err(FundForgeError::ClientSideErrorDebug(format!("Receiver error at callback recv: {}", e)))
                }
            }
        }
    }

    pub async fn accounts(&self) -> Result<Vec<AccountId>, FundForgeError> {
        let request = DataServerRequest::Accounts {
            callback_id: 0,
            brokerage: self.clone(),
        };
        let (sender, receiver) = oneshot::channel();
        let msg = StrategyRequest::CallBack(ConnectionType::Broker(self.clone()), request, sender);
        send_request(msg).await;
        match receiver.await {
            Ok(response) => match response {
                DataServerResponse::Accounts { accounts, .. } => Ok(accounts),
                DataServerResponse::Error { error, .. } => Err(error),
                _ => Err(FundForgeError::ClientSideErrorDebug("Incorrect response received at callback".to_string()))
            },
            Err(e) => Err(FundForgeError::ClientSideErrorDebug(format!("Receiver error at callback recv: {}", e)))
        }
    }

    pub async fn symbol_names(&self, callback_id: u64, time: Option<DateTime<Utc>>) -> Result<Vec<SymbolName>, FundForgeError> {
        match self {
            Brokerage::Rithmic(_) => Ok(get_available_rithmic_symbol_names().clone()),
            Brokerage::Oanda => Ok(get_oanda_symbol_names().clone()),
            _ => {
                let time = match time {
                    None => None,
                    Some(t) => Some(t.to_string())
                };
                let request = DataServerRequest::SymbolNames {
                    callback_id,
                    brokerage: self.clone(),
                    time
                };
                let (sender, receiver) = oneshot::channel();
                let msg = StrategyRequest::CallBack(ConnectionType::Broker(self.clone()), request, sender);
                send_request(msg).await;
                match receiver.await {
                    Ok(response) => match response {
                        DataServerResponse::SymbolNames { symbol_names, .. } => Ok(symbol_names),
                        DataServerResponse::Error { error, .. } => Err(error),
                        _ => Err(FundForgeError::ClientSideErrorDebug("Incorrect response received at callback".to_string()))
                    },
                    Err(e) => Err(FundForgeError::ClientSideErrorDebug(format!("Receiver error at callback recv: {}", e)))
                }
            }
        }
    }

    pub async fn commission_info(&self, symbol_name: SymbolName) -> Result<CommissionInfo, FundForgeError> {
        let request = DataServerRequest::CommissionInfo {
            callback_id: 0,
            brokerage: self.clone(),
            symbol_name,
        };
        let (sender, receiver) = oneshot::channel();
        let msg = StrategyRequest::CallBack(ConnectionType::Broker(self.clone()), request, sender);
        send_request(msg).await;
        match receiver.await {
            Ok(response) => match response {
                DataServerResponse::CommissionInfo { commission_info, .. } => Ok(commission_info),
                DataServerResponse::Error { error, .. } => Err(error),
                _ => Err(FundForgeError::ClientSideErrorDebug("Incorrect response received at callback".to_string()))
            },
            Err(e) => Err(FundForgeError::ClientSideErrorDebug(format!("Receiver error at callback recv: {}", e)))
        }
    }

    pub async fn account_info(&self, account_id: AccountId) -> Result<AccountInfo, FundForgeError> {
        let request = DataServerRequest::AccountInfo {
            callback_id: 0,
            brokerage: self.clone(),
            account_id
        };
        let (sender, receiver) = oneshot::channel();
        let msg = StrategyRequest::CallBack(ConnectionType::Broker(self.clone()), request, sender);
        send_request(msg).await;
        match receiver.await {
            Ok(response) => match response {
                DataServerResponse::AccountInfo { account_info, .. } => Ok(account_info),
                DataServerResponse::Error { error, .. } => Err(error),
                _ => Err(FundForgeError::ClientSideErrorDebug("Incorrect response received at callback".to_string()))
            },
            Err(e) => Err(FundForgeError::ClientSideErrorDebug(format!("Receiver error at callback recv: {}", e)))
        }
    }
}