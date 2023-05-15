use crate::error::ContractError;
use crate::state::PRICE;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128,
};
use cw_ownable::{initialize_owner, assert_owner, update_ownership, Action};
use fuzio_bet::fast_oracle::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    PRICE.save(deps.storage, &0u128)?;

    initialize_owner(
        deps.storage,
        deps.api,
        Some(&info.sender.clone().into_string()),
    )?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    
    assert_owner(deps.storage, &info.sender)?;

    match msg {
        ExecuteMsg::Update { price } => execute_set_price(deps, price),
        ExecuteMsg::Owner { owner } => execute_set_owner(deps, env, info, owner.to_string()),
    }
}

fn execute_set_owner(deps: DepsMut, env: Env, info: MessageInfo, owner: String) -> Result<Response, ContractError> {
    let owner = deps.api.addr_validate(&owner)?;

    update_ownership(deps, &env.block, &info.sender, Action::TransferOwnership { new_owner: owner.to_string(), expiry: None })?;

    Ok(Response::new())
}

fn execute_set_price(deps: DepsMut, price: Uint128) -> Result<Response, ContractError> {
    PRICE.save(deps.storage, &price.u128())?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Price {} => to_binary(&query_price(deps, env)?),
    }
}

fn query_price(deps: Deps, _env: Env) -> StdResult<Uint128> {
    let price: Uint128 = PRICE.load(deps.storage)?.into();

    Ok(price)
}
