use std::str::FromStr;
use std::sync::Arc;
#[allow(unused_imports)]
use ff_rithmic_api::credentials::RithmicCredentials;
#[allow(unused_imports)]
use ff_rithmic_api::rithmic_proto_objects::rti::{AccountListUpdates, AccountPnLPositionUpdate, AccountRmsUpdates, BestBidOffer, BracketUpdates, DepthByOrder, DepthByOrderEndEvent, EndOfDayPrices, ExchangeOrderNotification, FrontMonthContractUpdate, IndicatorPrices, InstrumentPnLPositionUpdate, LastTrade, MarketMode, OpenInterest, OrderBook, OrderPriceLimits, QuoteStatistics, RequestAccountList, RequestAccountRmsInfo, RequestHeartbeat, RequestLoginInfo, RequestMarketDataUpdate, RequestPnLPositionSnapshot, RequestPnLPositionUpdates, RequestProductCodes, RequestProductRmsInfo, RequestReferenceData, RequestTickBarUpdate, RequestTimeBarUpdate, RequestVolumeProfileMinuteBars, ResponseAcceptAgreement, ResponseAccountList, ResponseAccountRmsInfo, ResponseAccountRmsUpdates, ResponseAuxilliaryReferenceData, ResponseBracketOrder, ResponseCancelAllOrders, ResponseCancelOrder, ResponseDepthByOrderSnapshot, ResponseDepthByOrderUpdates, ResponseEasyToBorrowList, ResponseExitPosition, ResponseFrontMonthContract, ResponseGetInstrumentByUnderlying, ResponseGetInstrumentByUnderlyingKeys, ResponseGetVolumeAtPrice, ResponseGiveTickSizeTypeTable, ResponseHeartbeat, ResponseLinkOrders, ResponseListAcceptedAgreements, ResponseListExchangePermissions, ResponseListUnacceptedAgreements, ResponseLogin, ResponseLoginInfo, ResponseLogout, ResponseMarketDataUpdate, ResponseMarketDataUpdateByUnderlying, ResponseModifyOrder, ResponseModifyOrderReferenceData, ResponseNewOrder, ResponseOcoOrder, ResponseOrderSessionConfig, ResponsePnLPositionSnapshot, ResponsePnLPositionUpdates, ResponseProductCodes, ResponseProductRmsInfo, ResponseReferenceData, ResponseReplayExecutions, ResponseResumeBars, ResponseRithmicSystemInfo, ResponseSearchSymbols, ResponseSetRithmicMrktDataSelfCertStatus, ResponseShowAgreement, ResponseShowBracketStops, ResponseShowBrackets, ResponseShowOrderHistory, ResponseShowOrderHistoryDates, ResponseShowOrderHistoryDetail, ResponseShowOrderHistorySummary, ResponseShowOrders, ResponseSubscribeForOrderUpdates, ResponseSubscribeToBracketUpdates, ResponseTickBarReplay, ResponseTickBarUpdate, ResponseTimeBarReplay, ResponseTimeBarUpdate, ResponseTradeRoutes, ResponseUpdateStopBracketLevel, ResponseUpdateTargetBracketLevel, ResponseVolumeProfileMinuteBars, RithmicOrderNotification, SymbolMarginRate, TickBar, TimeBar, TradeRoute, TradeStatistics, UpdateEasyToBorrowList};
use ff_rithmic_api::rithmic_proto_objects::rti::Reject;
use ff_rithmic_api::rithmic_proto_objects::rti::request_login::SysInfraType;
use prost::{Message as ProstMessage};
use rust_decimal::{Decimal};
use ff_standard_lib::messages::data_server_messaging::DataServerResponse;
#[allow(unused_imports)]
use ff_standard_lib::standardized_types::broker_enum::Brokerage;
use crate::rithmic_api::api_client::RithmicClient;
use crate::rithmic_api::plant_handlers::handler_loop::send_updates;

#[allow(dead_code, unused)]
pub async fn match_pnl_plant_id(
    template_id: i32, message_buf: Vec<u8>,
    client: Arc<RithmicClient>
) {
    const PLANT: SysInfraType = SysInfraType::PnlPlant;
    match template_id {
        75 => {
            if let Ok(msg) = Reject::decode(&message_buf[..]) {
                // Login Response
                // From Server
                println!("Reject Response (Template ID: 11) from Server: {:?}", msg);
            }
        }
        11 => {
            if let Ok(msg) = ResponseLogin::decode(&message_buf[..]) {
                // Login Response
                // From Server
                println!("Login Response (Template ID: 11) from Server: {:?}", msg);
            }
        },
        13 => {
            if let Ok(msg) = ResponseLogout::decode(&message_buf[..]) {
                // Logout Response
                // From Server
                println!("Logout Response (Template ID: 13) from Server: {:?}", msg);
            }
        },
        15 => {
            if let Ok(msg) = ResponseReferenceData::decode(&message_buf[..]) {
                // Reference Data Response
                // From Server
                println!("Reference Data Response (Template ID: 15) from Server: {:?}", msg);
            }
        },
        17 => {
            if let Ok(msg) = ResponseRithmicSystemInfo::decode(&message_buf[..]) {
                // Rithmic System Info Response
                // From Server
                println!("Rithmic System Info Response (Template ID: 17) from Server: {:?}", msg);
            }
        },
        19 => {
            if let Ok(msg) = ResponseHeartbeat::decode(&message_buf[..]) {
                // Response Heartbeat
                // From Server
                //println!("Response Heartbeat (Template ID: 19) from Server: {:?}", msg);
                client.handle_response_heartbeat(PLANT, msg);
            }
        },
        401 => {
            if let Ok(msg) = ResponsePnLPositionUpdates::decode(&message_buf[..]) {
                // PnL Position Updates Response
                // From Server
                println!("PnL Position Updates Response (Template ID: 401) from Server: {:?}", msg);
            }
        },
        403 => {
            if let Ok(msg) = ResponsePnLPositionSnapshot::decode(&message_buf[..]) {
                // PnL Position Snapshot Response
                // From Server
                println!("PnL Position Snapshot Response (Template ID: 403) from Server: {:?}", msg);

            }
        },
        450 => {
            if let Ok(msg) = InstrumentPnLPositionUpdate::decode(&message_buf[..]) {
                // Instrument PnL Position Update
                // From Server
                println!("Instrument PnL Position Update (Template ID: 450) from Server: {:?}", msg);

                /*
                    Account PnL Position Update (Template ID: 451) from Server: AccountPnLPositionUpdate { template_id: 451, is_snapshot: None, fcm_id: Some("TopstepTrader"),
                    ib_id: Some("TopstepTrader"), account_id: Some("S1Sep246906077"), fill_buy_qty: Some(1), fill_sell_qty: Some(0), order_buy_qty: Some(0), order_sell_qty: Some(0),
                    buy_qty: Some(1), sell_qty: Some(-1), open_long_options_value: Some("0.00"), open_short_options_value: Some("0.00"), closed_options_value: Some("0.00"),
                    option_cash_reserved: Some("0.00"), rms_account_commission: None, open_position_pnl: Some("-2.00"), open_position_quantity: Some(1),
                    closed_position_pnl: Some("0.00"), closed_position_quantity: Some(0), net_quantity: Some(1), excess_buy_margin: Some(""), margin_balance: Some(""),
                    min_margin_balance: Some(""), min_account_balance: Some("0.00"), account_balance: Some("49989.80"), cash_on_hand: Some("49992.40"),
                    option_closed_pnl: Some("0.00"), percent_maximum_allowable_loss: Some("0.26"), option_open_pnl: Some("0.00"), mtm_account: Some("-2.60"),
                    available_buying_power: Some(""), used_buying_power: Some(""), reserved_buying_power: Some(""), excess_sell_margin: Some(""),
                    day_open_pnl: Some("-2.00"), day_closed_pnl: Some("0.00"), day_pnl: Some("-2.60"), day_open_pnl_offset: Some("0.00"),
                    day_closed_pnl_offset: Some("0.00"), ssboe: Some(1729085252), usecs: Some(932000) }
                */


            }
        },
        451 => {
            if let Ok(msg) = AccountPnLPositionUpdate::decode(&message_buf[..]) {
                // Account PnL Position Update
                // From Server
                //println!("Account PnL Position Update (Template ID: 451) from Server: {:?}", msg);
                /*Account PnL Position Update (Template ID: 451) from Server: AccountPnLPositionUpdate {
                template_id: 451, is_snapshot: Some(true), fcm_id: Some("TopstepTrader"),
                ib_id: Some("TopstepTrader"), account_id: Some("S1Sep246906077"), fill_buy_qty: Some(0),
                fill_sell_qty: Some(0), order_buy_qty: Some(0), order_sell_qty: Some(0), buy_qty: Some(0), sell_qty: Some(0),
                open_long_options_value: Some("0.00"), open_short_options_value: Some("0.00"),
                closed_options_value: Some("0.00"), option_cash_reserved: Some("0.00"), rms_account_commission: None,
                open_position_pnl: Some("0.00"), open_position_quantity: Some(0), closed_position_pnl: Some("0.00"),
                closed_position_quantity: Some(0), net_quantity: Some(0), excess_buy_margin: Some(""), margin_balance: Some(""),
                min_margin_balance: Some(""), min_account_balance: Some("0.00"), account_balance: Some("49992.40"), cash_on_hand: Some("49992.40"),
                option_closed_pnl: Some("0.00"), percent_maximum_allowable_loss: Some(""), option_open_pnl: Some("0.00"), mtm_account: Some("0.00"),
                available_buying_power: Some(""), used_buying_power: Some(""), reserved_buying_power: Some(""), excess_sell_margin: Some(""),
                day_open_pnl: Some("0.00"), day_closed_pnl: Some("0.00"), day_pnl: Some("0.00"), day_open_pnl_offset: Some("0.00"),
                day_closed_pnl_offset: Some("0.00"), ssboe: Some(1729083215), usecs: Some(883000) }*/

                let id = match msg.account_id {
                    None => return,
                    Some(id) => id
                };

                //match msg.

                match msg.account_balance {
                    None => {},
                    Some(account_balance) => {
                        match Decimal::from_str(&account_balance) {
                            Ok(account_balance) =>{
                                client.account_balance.insert(id.clone(), account_balance);
                            },
                            Err(_) => {}
                        }
                    }
                };

                match msg.cash_on_hand {
                    None => {},
                    Some(cash_on_hand) => {
                        match Decimal::from_str(&cash_on_hand) {
                            Ok(cash_on_hand) => {
                                client.account_cash_available.insert(id.clone(), cash_on_hand);
                            },
                            Err(_) => {}
                        }
                    }
                };

                match msg.open_position_pnl {
                    None => {},
                    Some(open_pnl) => {
                        match Decimal::from_str(&open_pnl) {
                            Ok(open_pnl) => {
                                client.open_pnl.insert(id.clone(), open_pnl);
                            },
                            Err(_) => {}
                        }
                    }
                };

                match msg.closed_position_pnl {
                    None => {},
                    Some(closed_position_pnl) => {
                        match Decimal::from_str(&closed_position_pnl) {
                            Ok(closed_position_pnl) => {
                                client.closed_pnl.insert(id.clone(), closed_position_pnl);
                            },
                            Err(_) => {}
                        }
                    }
                };

                match msg.day_closed_pnl {
                    None => {},
                    Some(day_closed_pnl) => {
                        match Decimal::from_str(&day_closed_pnl) {
                            Ok(day_closed_pnl) => {
                                client.day_closed_pnl.insert(id.clone(), day_closed_pnl);
                            },
                            Err(_) => {}
                        }
                    }
                };

                match msg.day_open_pnl {
                    None => {},
                    Some(day_open_pnl) => {
                        match Decimal::from_str(&day_open_pnl) {
                            Ok(day_open_pnl) => {
                                client.day_open_pnl.insert(id.clone(), day_open_pnl);
                            },
                            Err(_) => {}
                        }
                    }
                };
                if let (Some(cash_value), Some(cash_available)) = (
                    client.account_balance.get(&id).map(|r| *r),
                    client.account_cash_available.get(&id).map(|r| *r)
                ) {
                    let cash_used = cash_value - cash_available;

                    // Update the cash_used in the DashMap
                    client.account_cash_used.insert(id.clone(), cash_used);

                    send_updates(DataServerResponse::LiveAccountUpdates {
                        brokerage: client.brokerage.clone(),
                        account_id: id,
                        cash_value,
                        cash_available,
                        cash_used,
                    }).await;
                }
            }
        },
        _ => println!("No match for template_id: {}", template_id)
    }
}
