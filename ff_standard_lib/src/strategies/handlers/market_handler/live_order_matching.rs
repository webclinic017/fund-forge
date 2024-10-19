use std::sync::Arc;
use chrono::{Utc};
use dashmap::DashMap;
use tokio::sync::mpsc::{Receiver};
use crate::standardized_types::orders::{Order, OrderId, OrderState, OrderUpdateEvent, OrderUpdateType};
use crate::strategies::client_features::server_connections::add_buffer;
use crate::strategies::strategy_events::StrategyEvent;

//todo, this probably isnt needed

pub fn live_order_update(
    open_order_cache: Arc<DashMap<OrderId, Order>>, //todo, make these static or lifetimes if possible.. might not be optimal though, look it up!
    closed_order_cache: Arc<DashMap<OrderId, Order>>,
    mut order_event_receiver: Receiver<OrderUpdateEvent>
) {
    tokio::task::spawn(async move {
        while let Some(order_update_event) = order_event_receiver.recv().await {
            match order_update_event {
                OrderUpdateEvent::OrderAccepted { brokerage, account_id, symbol_name, symbol_code, order_id, tag, time } => {
                    if let Some(mut order) = open_order_cache.get_mut(&order_id) {
                        order.value_mut().state = OrderState::Accepted;
                        order.symbol_code = Some(symbol_code.clone());
                        let event = StrategyEvent::OrderEvents(OrderUpdateEvent::OrderAccepted { symbol_name, symbol_code, order_id, account_id: account_id.clone(), brokerage, tag, time });
                        add_buffer(Utc::now(), event).await;
                    }
                }
                OrderUpdateEvent::OrderFilled { brokerage: _, account_id: _, symbol_name: _, symbol_code: _, order_id: _, price: _, quantity: _, tag: _, time: _ } => {
                   //todo send direct via LEDGER_SERVICE
                    /* if let Some((order_id, mut order)) = open_order_cache.remove(&order_id) {
                        order.symbol_code = Some(symbol_code.clone());
                        order.state = OrderState::Filled;
                        order.quantity_filled += quantity;
                        order.time_filled_utc = Some(time.clone());
                        let event = StrategyEvent::OrderEvents(OrderUpdateEvent::OrderFilled { order_id: order_id.clone(), price, account_id: account_id.clone(), symbol_name, brokerage, tag, time: time.clone(), quantity, symbol_code: symbol_code.clone() });
                        add_buffer(Utc::now(), event).await;
                        if let Some(broker_map) = ledger_senders.get(&order.brokerage) {
                            if let Some(account_map) = broker_map.get_mut(&order.account_id) {
                                let symbol_code = match &order.symbol_code {
                                    None => order.symbol_name.clone(),
                                    Some(code) => code.clone()
                                };
                                let ledger_message = LedgerMessage::UpdateOrCreatePosition {symbol_name: order.symbol_name.clone(), symbol_code, order_id: order_id.clone(), quantity: order.quantity_filled, side: order.side.clone(), time: Utc::now(), market_fill_price: order.average_fill_price.unwrap(), tag: order.tag.clone()};
                                match account_map.value().send(ledger_message).await {
                                    Ok(_) => {}
                                    Err(e) => eprintln!("Error Sender Ledger Message in backtest_matching_engine::fill_order(), {}", e)
                                }
                            }
                        }
                        closed_order_cache.insert(order_id.clone(), order);
                    }*/
                }
                OrderUpdateEvent::OrderPartiallyFilled { brokerage: _, account_id: _, symbol_name: _, symbol_code: _, order_id: _, price: _, quantity: _, tag: _, time: _ } => {
               /*     if let Some(mut order) = open_order_cache.get_mut(&order_id) {
                        order.state = OrderState::PartiallyFilled;
                        order.symbol_code = Some(symbol_code.clone());
                        order.quantity_filled += quantity;
                        order.time_filled_utc = Some(time.clone());
                        let event = StrategyEvent::OrderEvents(OrderUpdateEvent::OrderFilled { order_id: order_id.clone(), price, account_id: account_id.clone(), symbol_name: symbol_name.clone(), brokerage, tag, time: time.clone(), quantity, symbol_code: symbol_code.clone() });
                        add_buffer(Utc::now(), event).await;
                        if let Some(broker_map) = ledger_senders.get(&order.brokerage) {
                            if let Some(account_map) = broker_map.get_mut(&order.account_id) {
                                let symbol_code = match &order.symbol_code {
                                    None => order.symbol_name.clone(),
                                    Some(code) => code.clone()
                                };
                                let ledger_message = LedgerMessage::UpdateOrCreatePosition {symbol_name: order.symbol_name.clone(), symbol_code, order_id: order_id.clone(), quantity: order.quantity_filled, side: order.side.clone(), time: Utc::now(), market_fill_price: order.average_fill_price.unwrap(), tag: order.tag.clone()};
                                match account_map.value().send(ledger_message).await {
                                    Ok(_) => {}
                                    Err(e) => eprintln!("Error Sender Ledger Message in backtest_matching_engine::fill_order(), {}", e)
                                }
                            }
                        }
                    }*/
                }
                OrderUpdateEvent::OrderCancelled { brokerage, account_id, symbol_name, symbol_code, order_id, tag, time } => {
                    if let Some((order_id, mut order)) = open_order_cache.remove(&order_id) {
                        order.state = OrderState::Cancelled;
                        order.symbol_code = Some(symbol_code.clone());
                        let event = StrategyEvent::OrderEvents(OrderUpdateEvent::OrderCancelled { order_id: order_id.clone(), account_id: account_id.clone(), symbol_name, brokerage, tag, time, symbol_code });
                        add_buffer(Utc::now(), event).await;
                        closed_order_cache.insert(order_id.clone(), order);
                    }
                }
                OrderUpdateEvent::OrderRejected { brokerage, account_id, symbol_name, symbol_code, order_id, reason, tag, time } => {
                    if let Some((order_id, mut order)) = open_order_cache.remove(&order_id) {
                        order.state = OrderState::Rejected(reason.clone());
                        order.symbol_code = Some(symbol_code.clone());
                        let event = StrategyEvent::OrderEvents(OrderUpdateEvent::OrderRejected { order_id: order_id.clone(), account_id, symbol_name, brokerage, reason, tag, time, symbol_code: symbol_code });
                        add_buffer(Utc::now(), event).await;
                        closed_order_cache.insert(order_id.clone(), order);
                    }
                }
                OrderUpdateEvent::OrderUpdated { brokerage, account_id, symbol_name, symbol_code, order_id, update_type, tag, time } => {
                    if let Some((id, mut order)) = open_order_cache.remove(&order_id) {
                        order.symbol_code = Some(symbol_code.clone());
                        match &update_type {
                            OrderUpdateType::LimitPrice(price) => order.limit_price = Some(price.clone()),
                            OrderUpdateType::TriggerPrice(price) => order.trigger_price = Some(price.clone()),
                            OrderUpdateType::TimeInForce(tif) => order.time_in_force = tif.clone(),
                            OrderUpdateType::Quantity(quantity) => order.quantity_open = quantity.clone(),
                            OrderUpdateType::Tag(tag) => order.tag = tag.clone()
                        }
                        open_order_cache.insert(id, order);
                    }
                    let event = StrategyEvent::OrderEvents(OrderUpdateEvent::OrderUpdated { order_id, account_id, symbol_name, brokerage, tag, time, update_type, symbol_code });
                    add_buffer(Utc::now(), event).await;
                }
                OrderUpdateEvent::OrderUpdateRejected { brokerage, account_id, order_id, reason, time } => {
                    if let Some((order_id, order)) = open_order_cache.remove(&order_id) {
                        closed_order_cache.insert(order_id.clone(), order);
                    }
                    let event = StrategyEvent::OrderEvents(OrderUpdateEvent::OrderUpdateRejected {order_id, account_id, brokerage, reason, time});
                    add_buffer(Utc::now(), event).await;
                }
            }
        }
    });
}