use bitcoin::{address::FromScriptError, consensus::encode::FromHexError, hex::HexToArrayError};
use corepc_client::client_sync::Error as ClientError;
use corepc_types::v24::GetRawTransactionVerboseError;
use std::fmt;

#[derive(Debug)]
pub enum AppError {
    Io(std::io::Error),
    Client(ClientError),
    AddressParse(bitcoin::address::ParseError),
    TxidParse(HexToArrayError),
    FromHex(FromHexError),
    AddressFromScript(FromScriptError),
    GetRawTx(GetRawTransactionVerboseError),
    MissingValue(String),
    State(String),
    Application(&'static str),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io(e) => write!(f, "IO error: {e}"),
            AppError::Client(e) => write!(f, "corepc-client error: {e}"),
            AppError::AddressParse(e) => write!(f, "Address formatting error: {e}"),
            AppError::AddressFromScript(e) => write!(f, "Address from script error: {e}"),
            AppError::TxidParse(e) => write!(f, "Hex parsing error: {e}"),
            AppError::FromHex(e) => write!(f, "Hex error: {e}"),
            AppError::GetRawTx(e) => write!(f, "Get raw transaction error: {e}"),
            AppError::MissingValue(msg) => write!(f, "Missing expected value: {msg}"),
            AppError::State(msg) => write!(f, "Unexpcted state error: {msg}"),
            AppError::Application(msg) => write!(f, "Application error: {msg}"),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppError::Io(e) => Some(e),
            AppError::Client(e) => Some(e),
            AppError::AddressParse(e) => Some(e),
            AppError::AddressFromScript(e) => Some(e),
            AppError::TxidParse(e) => Some(e),
            AppError::FromHex(e) => Some(e),
            AppError::GetRawTx(e) => Some(e),
            AppError::MissingValue(_) => None,
            AppError::State(_) => None,
            AppError::Application(_) => None,
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err)
    }
}

impl From<ClientError> for AppError {
    fn from(err: ClientError) -> Self {
        AppError::Client(err)
    }
}

impl From<bitcoin::address::ParseError> for AppError {
    fn from(err: bitcoin::address::ParseError) -> Self {
        AppError::AddressParse(err)
    }
}

impl From<FromScriptError> for AppError {
    fn from(err: FromScriptError) -> Self {
        AppError::AddressFromScript(err)
    }
}

impl From<HexToArrayError> for AppError {
    fn from(err: HexToArrayError) -> Self {
        AppError::TxidParse(err)
    }
}

impl From<FromHexError> for AppError {
    fn from(err: FromHexError) -> Self {
        AppError::FromHex(err)
    }
}

impl From<GetRawTransactionVerboseError> for AppError {
    fn from(err: GetRawTransactionVerboseError) -> Self {
        AppError::GetRawTx(err)
    }
}

impl From<&'static str> for AppError {
    fn from(err: &'static str) -> Self {
        AppError::Application(err)
    }
}
