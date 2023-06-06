use cw0::PaymentError;
use thiserror::Error;

use cosmwasm_std::StdError;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Payment(#[from] PaymentError),
    
    #[error("Wrong denom sent")]
    InvalidFunds {},

    #[error("Not enough funds for the bet sent")]
    NotEnoughFunds {},

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("The sum of wallet ratio is not equal to 1")]
    WrongRatio {},

    #[error("Denom not found in oracle")]
    PriceNotFoundInOracle {},

    #[error("At least one admin must remain")]
    NeedOneAdmin {},
}
