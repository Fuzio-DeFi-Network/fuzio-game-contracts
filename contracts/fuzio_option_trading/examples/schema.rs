use cosmwasm_schema::write_api;
use fuzio_bet::fuzio_option_trading::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, MigrateMsg};

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        execute: ExecuteMsg,
        query: QueryMsg,
        migrate: MigrateMsg,
    }
}
