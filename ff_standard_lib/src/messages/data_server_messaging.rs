use crate::standardized_types::enums::{MarketType, OrderSide, StrategyMode, PrimarySubscription, PositionSide, FuturesExchange};
use crate::standardized_types::subscriptions::{DataSubscription, Symbol, SymbolCode, SymbolName};
use crate::standardized_types::bytes_trait::Bytes;
use rkyv::{Archive, Deserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};
use std::fmt::{Debug, Display};
use rkyv::ser::Serializer;
use rust_decimal::Decimal;
use crate::standardized_types::accounts::{Account, AccountId, AccountInfo, Currency};
use crate::standardized_types::broker_enum::Brokerage;
use crate::standardized_types::datavendor_enum::DataVendor;
use crate::standardized_types::base_data::base_data_type::BaseDataType;
use crate::standardized_types::new_types::{Price};
use crate::standardized_types::orders::{OrderRequest, OrderUpdateEvent};
use crate::standardized_types::symbol_info::{CommissionInfo, FrontMonthInfo, SymbolInfo};

/// An Api key String
pub type ApiKey = String;

#[derive(Clone, Serialize, Deserialize, Archive, Debug, PartialEq)]
#[archive(compare(PartialEq), check_bytes)]
#[archive_attr(derive(Debug))]
pub enum StreamResponse {
    SubscribeBaseData(DataSubscription),
    CreateConsolidator{primary: DataSubscription, secondary: DataSubscription},
    UnSubscribeBaseData(DataSubscription),
}

#[derive(Clone, Serialize, Deserialize, Archive, Debug, SerdeSerialize, SerdeDeserialize, PartialEq, Eq, PartialOrd, Ord, )]
#[archive(compare(PartialEq), check_bytes)]
#[archive_attr(derive(Debug))]
pub struct AccountState {
    balance: Decimal,
    equity_used: Decimal,
    equity_available: Decimal
}

#[derive(Clone, Serialize, Deserialize, Archive, Debug, PartialEq, Eq, PartialOrd, Ord, )]
#[archive(compare(PartialEq), check_bytes)]
#[archive_attr(derive(Debug))]
pub enum StreamRequest {
    Subscribe(DataSubscription),
    Unsubscribe(DataSubscription)
}

#[derive(Clone, Serialize, Deserialize, Archive, Debug)]
#[archive(compare(PartialEq), check_bytes, )]
#[archive_attr(derive(Debug))]
/// Represents a request type for the network message. This enum is used to specify the type of request and the returning response
///
/// # Variants
/// * [`SynchronousRequestType::HistoricalBaseData`](ff_data_vendors::networks::RequestType) : Requests the Base data for the specified subscriptions. Server returns a ResponseType::HistoricalBaseData with the data payload.
pub enum DataServerRequest {
    Register(StrategyMode),

    /// Requests a list of instruments all instruments available with the `DataVendor` from the server, an instrument object is the vendors specific data type.
    /// # Fields
    /// * `DataVendor`
    /// * `MarketType`
    SymbolsVendor {
        callback_id: u64,
        data_vendor: DataVendor,
        market_type: MarketType,
        time: Option<String>
    },
    BaseDataTypes {
        callback_id: u64,
        data_vendor: DataVendor
    },
    /// Requests a list of resolutions available with the `DataVendor` from the server
    Resolutions {
        callback_id: u64,
        data_vendor: DataVendor,
        market_type: MarketType
    },
    WarmUpResolutions {
        callback_id: u64,
        data_vendor: DataVendor,
        market_type: MarketType
    },
    AccountInfo {
        callback_id: u64,
        brokerage: Brokerage,
        account_id: AccountId
    },
    Markets {
        callback_id: u64,
        data_vendor: DataVendor
    },
    TickSize {
        callback_id: u64,
        data_vendor: DataVendor,
        symbol_name: SymbolName
    },
    DecimalAccuracy {
        callback_id: u64,
        data_vendor: DataVendor,
        symbol_name: SymbolName
    },
    SymbolInfo{
        callback_id: u64,
        brokerage: Brokerage,
        symbol_name: SymbolName
    },
    StreamRequest {
        request: StreamRequest
    },
    OrderRequest {
        request: OrderRequest
    },
    PrimarySubscriptionFor {
        callback_id: u64,
        subscription: DataSubscription
    },
    CommissionInfo{
        callback_id: u64,
        brokerage: Brokerage,
        symbol_name: SymbolName
    },
    ExchangeRate {
        callback_id: u64,
        from_currency: Currency,
        to_currency: Currency,
        date_time_string: String,
        data_vendor: DataVendor,
        side: OrderSide
    },
    GetCompressedHistoricalData {
        callback_id: u64,
        subscriptions: Vec<DataSubscription>,
        from_time: String,
        to_time: String
    },
    FrontMonthInfo {
        callback_id: u64,
        symbol_name: SymbolName,
        exchange: FuturesExchange,
        brokerage: Brokerage
    },
    Accounts{callback_id: u64, brokerage: Brokerage},
    SymbolNames{callback_id: u64, brokerage: Brokerage, time: Option<String>},
    RegisterStreamer{port: u16, secs: u64, subsec: u32},
}

impl DataServerRequest {
    pub fn to_bytes(&self) -> Vec<u8> {
        let vec = rkyv::to_bytes::<_, 1024>(self).unwrap();
        vec.into()
    }
    pub fn from_bytes(archived: &[u8]) -> Result<DataServerRequest, FundForgeError> {
        // If the archived bytes do not end with the delimiter, proceed as before
        match rkyv::from_bytes::<DataServerRequest>(archived) {
            //Ignore this warning: Trait `Deserialize<RequestType, SharedDeserializeMap>` is not implemented for `ArchivedRequestType` [E0277]
            Ok(response) => Ok(response),
            Err(e) => Err(FundForgeError::ClientSideErrorDebug(e.to_string())),
        }
    }
    pub fn set_callback_id(&mut self, id: u64) {
        match self {
            DataServerRequest::SymbolsVendor { callback_id, .. } => {*callback_id = id}
            DataServerRequest::Resolutions {callback_id, .. } => {*callback_id = id}
            DataServerRequest::AccountInfo { callback_id, .. } => {*callback_id = id}
            DataServerRequest::BaseDataTypes { callback_id, .. } => {*callback_id = id}
            DataServerRequest::Markets { callback_id, .. } => {*callback_id = id}
            DataServerRequest::TickSize { callback_id, .. } => {*callback_id = id}
            DataServerRequest::DecimalAccuracy { callback_id, .. } => {*callback_id = id}
            DataServerRequest::SymbolInfo { callback_id, .. } => {*callback_id = id}
            DataServerRequest::StreamRequest   { .. } => {}
            DataServerRequest::Register {  .. } => {}
            DataServerRequest::OrderRequest { .. } => {}
            DataServerRequest::Accounts { callback_id, .. } => {*callback_id = id}
            DataServerRequest::PrimarySubscriptionFor { callback_id, .. } => {*callback_id = id}
            DataServerRequest::SymbolNames { callback_id, .. } => {*callback_id = id}
            DataServerRequest::RegisterStreamer{..} => {}
            DataServerRequest::CommissionInfo { callback_id, .. } => {*callback_id = id}
            DataServerRequest::WarmUpResolutions { callback_id, .. } => {*callback_id = id}
            DataServerRequest::ExchangeRate { callback_id, .. } => {*callback_id = id}
            DataServerRequest::GetCompressedHistoricalData { callback_id, .. } => {*callback_id = id}
            DataServerRequest::FrontMonthInfo { callback_id, .. } => {*callback_id = id}
        }
    }
}

//todo, could do something like this
pub enum SubscriptionResponse {
    CreateConsolidator{callback_id: u64, base_subscription: DataSubscription},
    Subscribed{callback_id: u64},
    UnableToSubscribe{callback_id: u64}
}

#[derive(Clone, Serialize, Deserialize, Archive, Debug)]
#[archive(compare(PartialEq), check_bytes)]
#[archive_attr(derive(Debug))]
/// Represents a request type for the network message. This enum is used to specify the type of request and the returning response
pub enum
DataServerResponse {
    CompressedHistoricalData {
        callback_id: u64,
        payload: Vec<Vec<u8>>
    },

    /// Responds with `instruments` as `Vec<InstrumentEnum>` which contains:
    /// *  `Vec<Symbol>` for all symbols available on the server, to fullfill this the vendor will need a fn that converts from its instrument format into a `Symbol` object.
    Symbols {
        callback_id: u64,
        symbols: Vec<Symbol>,
        market_type: MarketType
    },

    BaseDataTypes {
        callback_id: u64,
        base_data_types: Vec<BaseDataType>
    },

/*    Rates{
        callback_id: u64,
        rates: BTreeMap<(Currency, Currency), Decimal>
    },*/

    /// Responds with a vec<(Resolution, BaseDataType)> which represents all the native resolutions available for the data types from the vendor api (note we only support intraday resolutions, higher resolutions are consolidated by the engine)
    Resolutions {
        callback_id: u64,
        subscription_resolutions_types: Vec<PrimarySubscription>,
        market_type: MarketType
    },

    ExchangeRate {
        callback_id: u64,
        rate: Decimal,
    },

    /// Provides the client with an error message
    /// Contains a `FundForgeError` which is used to help debug and identify the type of error that occurred.
    /// [`DataServerError`](ff_data_vendors::networks::DataServerError)
    Error {
        callback_id: u64,
        error: FundForgeError
    },

    AsyncError {
        error: FundForgeError
    },

    AccountInfo {
        callback_id: u64,
        account_info: AccountInfo
    },

    Markets {
        callback_id: u64,
        markets: Vec<MarketType>
    },

    TickSize {
        callback_id: u64,
        tick_size: Price
    },

    DecimalAccuracy{
        callback_id: u64,
        accuracy: u32
    },

    ValuePerTick{
        callback_id: u64,
        currency: Currency,
        price: Price
    },

    SymbolInfo {
        callback_id: u64,
        symbol_info: SymbolInfo
    },

    SymbolInfoMany {
        callback_id: u64,
        info_vec: Vec<SymbolInfo>
    },

    SubscribeResponse {
        success: bool,
        subscription: DataSubscription,
        reason: Option<String>
    },

    UnSubscribeResponse {
        success: bool,
        subscription: DataSubscription,
        reason: Option<String>
    },

    FrontMonthInfo{
        callback_id: u64,
        info: FrontMonthInfo
    },

    SymbolNames{callback_id: u64, symbol_names: Vec<SymbolName>},

    Accounts{callback_id: u64, accounts: Vec<AccountId>},

    PrimarySubscriptionFor{callback_id: u64, primary_subscription: DataSubscription},

    OrderUpdates{event: OrderUpdateEvent, time: String},

    RegistrationResponse(u16),

    CommissionInfo{callback_id: u64, commission_info: CommissionInfo},

    LiveAccountUpdates {account: Account, cash_value: Decimal, cash_available: Decimal, cash_used: Decimal},

    /// Booked pnl is only sent for closed positions, it is the amount of booked pnl since the last side change from none to long or short
    LivePositionUpdates {symbol_name: SymbolName, symbol_code: SymbolCode, account: Account, open_quantity: f64, average_price: f64, side: PositionSide, open_pnl: f64, time: String},
}

impl Bytes<DataServerResponse> for DataServerResponse {
    fn from_bytes(archived: &[u8]) -> Result<DataServerResponse, FundForgeError> {
        // If the archived bytes do not end with the delimiter, proceed as before
        match rkyv::from_bytes::<DataServerResponse>(archived) {
            //Ignore this warning: Trait `Deserialize<ResponseType, SharedDeserializeMap>` is not implemented for `ArchivedRequestType` [E0277]
            Ok(response) => Ok(response),
            Err(e) => Err(FundForgeError::ClientSideErrorDebug(e.to_string())),
        }
    }
    fn to_bytes(&self) -> Vec<u8> {
        use rkyv::ser::serializers::AllocSerializer;
        let mut serializer = AllocSerializer::<0>::default();
        serializer.serialize_value(self).unwrap();
        serializer.into_serializer().into_inner().into()
    }
}

impl DataServerResponse {
    pub fn get_callback_id(&self) -> Option<u64> {
        match self {
            DataServerResponse::Symbols  { callback_id,.. } => Some(callback_id.clone()),
            DataServerResponse::Resolutions  { callback_id,.. } => Some(callback_id.clone()),
            DataServerResponse::Error  { callback_id,.. } => Some(callback_id.clone()),
            DataServerResponse::AccountInfo  { callback_id,.. } => Some(callback_id.clone()),
            DataServerResponse::Markets  { callback_id,.. } => Some(callback_id.clone()),
            DataServerResponse::TickSize  { callback_id,.. } => Some(callback_id.clone()),
            DataServerResponse::DecimalAccuracy  { callback_id,.. } => Some(callback_id.clone()),
            DataServerResponse::ValuePerTick  { callback_id,.. } => Some(callback_id.clone()),
            DataServerResponse::SymbolInfo  { callback_id,.. } => Some(callback_id.clone()),
            DataServerResponse::SymbolInfoMany  { callback_id,.. } => Some(callback_id.clone()),
            DataServerResponse::BaseDataTypes { callback_id,.. } => Some(callback_id.clone()),
            DataServerResponse::SubscribeResponse { .. } => None,
            DataServerResponse::UnSubscribeResponse { .. } => None,
            DataServerResponse::Accounts {callback_id, ..} => Some(callback_id.clone()),
            DataServerResponse::OrderUpdates{..} => None,
            DataServerResponse::PrimarySubscriptionFor {callback_id, ..} => Some(callback_id.clone()),
            DataServerResponse::SymbolNames {callback_id, ..} => Some(callback_id.clone()),
            DataServerResponse::RegistrationResponse(_) => None,
            DataServerResponse::CommissionInfo { callback_id,.. } => Some(callback_id.clone()),
            DataServerResponse::FrontMonthInfo { callback_id, .. } => Some(callback_id.clone()),
            DataServerResponse::LiveAccountUpdates { .. } => None,
            DataServerResponse::LivePositionUpdates { .. } => None,
            DataServerResponse::AsyncError { .. } => None,
            DataServerResponse::ExchangeRate { callback_id, .. } => Some(callback_id.clone()),
            DataServerResponse::CompressedHistoricalData { callback_id, .. } => Some(callback_id.clone()),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Archive, PartialEq)]
#[archive(compare(PartialEq), check_bytes)]
#[archive_attr(derive(Debug))]
/// Represents a response type for the network message. This is used to help debug and identify the type of error that occurred
/// # Variants
/// * `InvalidApiKey` - The vendor or broker API key used to authenticate the request is invalid. [`DataServerError::InvalidApiKey`](ff_data_vendors::networks::DataServerError)
/// * `InvalidRequestType` - The type of request being made is invalid. [`DataServerError::InvalidRequestType`](ff_data_vendors::networks::DataServerError)
/// * `ServerErrorDebug` - A server side error occurred, the debug message is provided as `String`. [`DataServerError::ServerErrorDebug`](ff_data_vendors::networks::DataServerError)
/// * `ClientSideErrorDebug` - A client side error occurred, the debug message is provided as `String`. [`DataServerError::ClientSideErrorDebug`](ff_data_vendors::networks::DataServerError)
pub enum FundForgeError {
    /// The API key used to authenticate the request is invalid.
    InvalidApiKey,
    /// The type of request being made is invalid.
    InvalidRequestType(String),
    /// A server side error occurred, the debug message is provided as `String`.
    ServerErrorDebug(String),
    /// A client side error occurred, the debug message is provided as `String`.
    ClientSideErrorDebug(String),
    /// An unknown error occurred, the blame is unknown.
    UnknownBlameError(String),
    /// An unknown error occurred, the debug message is provided as `String`.
    ConnectionNotFound(String),
}

impl Debug for FundForgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FundForgeError::InvalidApiKey => write!(f, "InvalidApiKey"),
            FundForgeError::InvalidRequestType(request_type) => {
                write!(f, "InvalidRequestType: {}", request_type)
            }
            FundForgeError::ServerErrorDebug(debug) => write!(f, "ServerErrorDebug: {}", debug),
            FundForgeError::ClientSideErrorDebug(debug) => {
                write!(f, "ClientSideErrorDebug: {}", debug)
            }
            FundForgeError::UnknownBlameError(debug) => write!(f, "UnknownBlameError: {}", debug),
            FundForgeError::ConnectionNotFound(debug) => write!(f, "ConnectionNotFound {}:", debug),
        }
    }
}

impl Display for FundForgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FundForgeError::InvalidApiKey => write!(f, "InvalidApiKey"),
            FundForgeError::InvalidRequestType(request_type) => {
                write!(f, "InvalidRequestType: {}", request_type)
            }
            FundForgeError::ServerErrorDebug(debug) => write!(f, "ServerErrorDebug: {}", debug),
            FundForgeError::ClientSideErrorDebug(debug) => {
                write!(f, "ClientSideErrorDebug: {}", debug)
            }
            FundForgeError::UnknownBlameError(debug) => write!(f, "UnknownBlameError: {}", debug),
            FundForgeError::ConnectionNotFound(debug) => {
                write!(f, "ConnectionNotFound: {}:", debug)
            }
        }
    }
}
