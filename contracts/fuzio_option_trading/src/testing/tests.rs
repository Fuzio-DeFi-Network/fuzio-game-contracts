use cosmwasm_std::{
    to_binary, Addr, Binary, BlockInfo, CosmosMsg, Empty, Response,
    StdResult, Timestamp, Uint128, WasmMsg, coins,
};
use cw_multi_test::{App, Contract, ContractWrapper, Executor};
use fuzio_bet::fast_oracle::{
    msg::ExecuteMsg as FastOracleExecuteMsg, msg::InstantiateMsg as FastOracleInstantiateMsg,
    msg::QueryMsg as FastOracleQueryMsg,
};
use fuzio_bet::fuzio_option_trading::{
    msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    ConfigResponse,
    Config,
};
use fuzio_bet::fuzio_option_trading::{Direction, RoundUsersResponse, PendingRewardResponse, ClaimInfoResponse};

use std::convert::TryInto;

pub fn contract_price_prediction() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );
    Box::new(contract)
}

pub fn contract_fast_oracle() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        |deps, _, _info, msg: FastOracleExecuteMsg| -> StdResult<Response> {
            match msg {
                FastOracleExecuteMsg::Update { price } => {
                    deps.storage.set(b"price", &price.to_be_bytes());
                    Ok(Response::default())
                }
                FastOracleExecuteMsg::Owner { owner: _ } => todo!(),
            }
        },
        |deps, _, _, _: FastOracleInstantiateMsg| -> StdResult<Response> {
            deps.storage
                .set(b"price", &Uint128::new(1_000_000u128).to_be_bytes());
            Ok(Response::default())
        },
        |deps, _, msg: FastOracleQueryMsg| -> StdResult<Binary> {
            match msg {
                FastOracleQueryMsg::Price {} => {
                    let res = deps.storage.get(b"price").unwrap_or_default();
                    let price =
                        Uint128::from(u128::from_be_bytes(res.as_slice().try_into().unwrap()));

                    to_binary(&price)
                }
            }
        },
    );
    Box::new(contract)
}

fn update_price(router: &mut App, config: ConfigResponse, price: Uint128, sender: &Addr) {
    let update_price_msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.fast_oracle_addr.to_string(),
        msg: to_binary(&FastOracleExecuteMsg::Update { price }).unwrap(),
        funds: vec![],
    });

    router
        .execute_multi(sender.clone(), [update_price_msg].to_vec())
        .unwrap();
}

fn start_next_round(router: &mut App, prediction_market_addr: &Addr, sender: &Addr) {
    router.update_block(|block| {
        block.time = block.time.plus_seconds(600);
        block.height += 1;
    });

    let start_live_round_msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: prediction_market_addr.to_string(),
        msg: to_binary(&ExecuteMsg::CloseRound {}).unwrap(),
        funds: vec![],
    });

    router
        .execute_multi(sender.clone(), [start_live_round_msg].to_vec())
        .unwrap();
}

fn init_fast_oracle_contract(router: &mut App, owner: &Addr) -> Addr {
    // println!("prediction_market_code_id, {:?}", prediction_market_code_id);

    let msg = FastOracleInstantiateMsg {};

    let fast_oracle_code_id = router.store_code(contract_fast_oracle());

    router
        .instantiate_contract(
            fast_oracle_code_id,
            Addr::unchecked("owner"),
            &msg,
            &[],
            "fast_oracle",
            Some(owner.to_string()),
        )
        .unwrap()
}

fn create_prediction_market(router: &mut App, owner: &Addr, config: Config) -> Addr {
    let prediction_market_code_id = router.store_code(contract_price_prediction());

    router.set_block(BlockInfo {
        height: 0,
        time: Timestamp::from_seconds(0),
        chain_id: "testing".to_string(),
    });

    let mut msg = InstantiateMsg {
        config: config.clone(),
    };

    let fast_oracle_addr: Addr = init_fast_oracle_contract(router, owner);

    msg.config.fast_oracle_addr = fast_oracle_addr;

    router
        .instantiate_contract(
            prediction_market_code_id,
            owner.clone(),
            &msg,
            &[],
            "prediction_market",
            Some(owner.to_string()),
        )
        .unwrap()
}

fn execute_bet(
    router: &mut App,
    user: Addr,
    amount: Uint128,
    direction: Direction,
    token_denom: String,
    prediction_market_addr: &Addr,
    round_id: Uint128,
) {
    let bet_msg: CosmosMsg;

    match direction {
        Direction::Bear => {
            bet_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: prediction_market_addr.to_string(),
                msg: to_binary(&ExecuteMsg::BetBear { amount, round_id }).unwrap(),
                funds: coins(amount.u128(), token_denom),
            });
        }
        Direction::Bull => {
            bet_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: prediction_market_addr.to_string(),
                msg: to_binary(&ExecuteMsg::BetBull { amount, round_id }).unwrap(),
                funds: coins(amount.u128(), token_denom),
            });
        }
    }

    router
        .execute_multi(user, [bet_msg].to_vec())
        .unwrap();
}

#[test]

fn test_bet() {

    let owner = Addr::unchecked("owner");
    let user1 = Addr::unchecked("user1");
    let user2 = Addr::unchecked("user2");

    
    let mut router = App::new(|router, _api, storage| {
        router
            .bank
            .init_balance(storage, &user1.clone(), coins(100000, "token"))
            .unwrap();
    });


    router.send_tokens(user1.clone(), user2.clone(), &coins(50000, "token"))
        .unwrap();

    let default_config: Config = Config {
        next_round_seconds: Uint128::new(600u128),
        fast_oracle_addr: Addr::unchecked("fast_oracle"),
        minimum_bet: Uint128::new(1u128),
        gaming_fee: Uint128::new(200u128),
        token_denom: "token".to_string(),
    };

    let prediction_market_addr =
        create_prediction_market(&mut router, &owner, default_config.clone());

    start_next_round(&mut router, &prediction_market_addr, &owner);

    let config: ConfigResponse = router
        .wrap()
        .query_wasm_smart(prediction_market_addr.to_string(), &QueryMsg::Config {})
        .unwrap();

    execute_bet(
        &mut router,
        user1.clone(),
        Uint128::new(100),
        Direction::Bear,
        config.clone().token_denom,
        &prediction_market_addr,
        Uint128::zero(),
    );

    execute_bet(
        &mut router,
        user2.clone(),
        Uint128::new(50),
        Direction::Bull,
        config.clone().token_denom,
        &prediction_market_addr,
        Uint128::zero(),
    );

    let round_users: RoundUsersResponse = router
        .wrap()
        .query_wasm_smart(
            prediction_market_addr.to_string(),
            &QueryMsg::GetUsersPerRound {
                round_id: Uint128::zero(),
                start_after: Some(user1.clone()),
                limit: None,
            },
        )
        .unwrap();

    println!("round users {:?}", round_users);

    //-----------------------------------------------------close the round and check the pending reward of first user-------------------------------------------

    // update_price(&mut router, config, price, sender)
    start_next_round(&mut router, &prediction_market_addr, &owner);
    update_price(&mut router, config, Uint128::new(50000), &owner);
    start_next_round(&mut router, &prediction_market_addr, &owner);

    let pending_reward_user1: PendingRewardResponse = router
        .wrap()
        .query_wasm_smart(
            prediction_market_addr.clone(),
            &QueryMsg::MyPendingRewardRound {
                round_id: Uint128::zero(),
                player: user1.clone(),
            },
        )
        .unwrap();
    let pending_reward_user2: PendingRewardResponse = router
        .wrap()
        .query_wasm_smart(
            prediction_market_addr.clone(),
            &QueryMsg::MyPendingRewardRound {
                round_id: Uint128::zero(),
                player: user2.clone(),
            },
        )
        .unwrap();

    println!(
        "pending reward for user1{:?}, pending reward for user2 {:?}",
        pending_reward_user1, pending_reward_user2
    );

    let claim_msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: prediction_market_addr.to_string(),
        msg: to_binary(&ExecuteMsg::CollectWinnings {}).unwrap(),
        funds: vec![],
    });

    router
        .execute_multi(user1.clone(), [claim_msg].to_vec())
        .unwrap();

    let claim_info: ClaimInfoResponse = router
        .wrap()
        .query_wasm_smart(
            prediction_market_addr.to_string(),
            &QueryMsg::GetClaimInfoByUser {
                player: user2,
                start_after: Some(Uint128::zero()),
                limit: None,
            },
        )
        .unwrap();
    println!("claim_info, {:?}", claim_info)
}
