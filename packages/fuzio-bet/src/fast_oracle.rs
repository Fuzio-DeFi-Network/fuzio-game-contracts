use cosmwasm_schema::cw_serde;
use cosmwasm_schema::QueryResponses;

pub mod msg {
    use cosmwasm_std::{Addr, Uint128};

    use super::*;

    #[cw_serde]
    pub struct InstantiateMsg {}

    #[cw_serde]
    pub enum ExecuteMsg {
        Update { price: Uint128 },
        Owner { owner: Addr },
    }

    #[cw_serde]
    #[derive(QueryResponses)]
    pub enum QueryMsg {
        #[returns(Uint128)]
        Price {},
    }
}
