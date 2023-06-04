use std::vec;

use crate::error::ContractError;
use crate::state::{
    bet_info_key, bet_info_storage, claim_info_key, claim_info_storage, ACCUMULATED_FEE, ADMINS,
    CONFIG, IS_HALTED, LIVE_ROUND, NEXT_ROUND, NEXT_ROUND_ID, ROUNDS, TOTALS_SPENT,
};
use cw0::one_coin;
use fuzio_bet::fuzio_option_trading::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use fuzio_bet::fuzio_option_trading::{
    AdminsResponse, BetInfo, ClaimInfo, ClaimInfoResponse, ConfigResponse, MyGameResponse,
    PendingRewardResponse, PendingRewardRoundsResponse, RoundUsersResponse, TotalSpentResponse,
    WalletInfo,
};
use fuzio_bet::fuzio_option_trading::{Config, Direction};

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_binary, Addr, BankMsg, Binary, Decimal, Deps, DepsMut, Env, Event, MessageInfo,
    Order, Response, StdError, StdResult, Uint128,
};
use cw_storage_plus::Bound;
use fuzio_bet::fuzio_option_trading::{FinishedRound, LiveRound, NextRound, FEE_PRECISION};
use fuzio_bet::fuzio_option_trading::{MyCurrentPositionResponse, StatusResponse};
use sei_cosmwasm::{ExchangeRatesResponse, SeiQuerier, SeiQueryWrapper};

// Query limits
const DEFAULT_QUERY_LIMIT: u32 = 10;
const MAX_QUERY_LIMIT: u32 = 30;

const CONTRACT_NAME: &str = "fuzio_option_trading";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let mut total_ratio = Decimal::zero();
    for dev_wallet in msg.config.dev_wallet_list.clone() {
        total_ratio = total_ratio + dev_wallet.ratio;
    }

    if total_ratio != Decimal::one() {
        return Err(ContractError::WrongRatio {});
    }

    CONFIG.save(deps.storage, &msg.config)?;
    NEXT_ROUND_ID.save(deps.storage, &0u128)?;
    ACCUMULATED_FEE.save(deps.storage, &0u128)?;
    IS_HALTED.save(deps.storage, &false)?;
    ADMINS.save(deps.storage, &vec![info.sender])?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, MigrateMsg {}: MigrateMsg) -> StdResult<Response> {
    let version = cw2::get_contract_version(deps.storage)?;
    if version.contract != CONTRACT_NAME {
        return Err(StdError::generic_err("Can only upgrade from same type"));
    }
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<SeiQueryWrapper>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig { config } => execute_update_config(deps, info, config),
        ExecuteMsg::BetBear { round_id, amount } => {
            execute_bet(deps, info, env, round_id, Direction::Bear, amount)
        }
        ExecuteMsg::BetBull { round_id, amount } => {
            execute_bet(deps, info, env, round_id, Direction::Bull, amount)
        }
        ExecuteMsg::CloseRound {} => execute_close_round(deps, env),
        ExecuteMsg::CollectWinnings {} => execute_collect_winnings(deps, info),
        ExecuteMsg::CollectionWinningRound { round_id } => {
            execute_collect_winning_round(deps, info, round_id)
        }
        ExecuteMsg::Halt {} => execute_update_halt(deps, info, true),
        ExecuteMsg::Resume {} => execute_update_halt(deps, info, false),
        ExecuteMsg::AddAdmin { new_admin } => execute_add_admin(deps, info, new_admin),
        ExecuteMsg::RemoveAdmin { old_admin } => execute_remove_admin(deps, info, old_admin),
        ExecuteMsg::ModifyDevWallet { new_dev_wallets } => {
            execute_modify_dev_wallets(deps, info, new_dev_wallets)
        }
    }
}

fn execute_collect_winnings(
    deps: DepsMut<SeiQueryWrapper>,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let mut winnings = Uint128::zero();
    let resp = Response::new();

    let my_game_list = query_my_games_without_limit(deps.as_ref(), info.sender.clone())?;
    let live_round = LIVE_ROUND.load(deps.storage)?;

    for game in my_game_list.my_game_list {
        let round_id = game.round_id;

        if live_round.id == round_id {
            continue;
        }

        let round = ROUNDS.load(deps.storage, round_id.u128())?;

        let pool_shares = round.bear_amount + round.bull_amount;
        let bet_info_key = bet_info_key(round_id.u128(), &info.sender);

        bet_info_storage().remove(deps.storage, bet_info_key.clone())?;

        let claim_info_key = claim_info_key(round_id.u128(), &info.sender);

        if round.bear_amount == Uint128::zero() || round.bull_amount == Uint128::zero() {
            winnings += game.amount;
            if game.amount > Uint128::zero() {
                claim_info_storage().save(
                    deps.storage,
                    claim_info_key,
                    &ClaimInfo {
                        player: info.sender.clone(),
                        round_id,
                        claimed_amount: winnings,
                    },
                )?;
            }
        } else {
            let round_winnings = match round.winner {
                Some(Direction::Bull) => {
                    /* Only claimable once */
                    match game.direction {
                        Direction::Bull => {
                            let won_shares = game.amount;
                            pool_shares.multiply_ratio(won_shares, round.bull_amount)
                        }
                        Direction::Bear => Uint128::zero(),
                    }
                }
                Some(Direction::Bear) => {
                    /* Only claimable once */
                    match game.direction {
                        Direction::Bull => Uint128::zero(),
                        Direction::Bear => {
                            let won_shares = game.amount;
                            pool_shares.multiply_ratio(won_shares, round.bear_amount)
                        }
                    }
                }
                None => {
                    /* Only claimable once */
                    game.amount
                }
            };

            /* Count it up */
            winnings += round_winnings;

            if round_winnings > Uint128::zero() {
                claim_info_storage().save(
                    deps.storage,
                    claim_info_key,
                    &ClaimInfo {
                        player: info.sender.clone(),
                        round_id,
                        claimed_amount: winnings,
                    },
                )?;
            }
        }
    }

    if winnings == Uint128::zero() {
        return Err(ContractError::Std(StdError::generic_err(
            "Nothing to claim",
        )));
    }

    let msg_send_winnings = BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: coins(winnings.u128(), &config.token_denom),
    };

    Ok(resp
        .add_message(msg_send_winnings)
        .add_attribute("action", "collect-winnings")
        .add_attribute("amount", winnings))
}

fn execute_collect_winning_round(
    deps: DepsMut<SeiQueryWrapper>,
    info: MessageInfo,
    round_id: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let mut winnings = Uint128::zero();
    let resp = Response::new();

    let mut my_game_list: Vec<BetInfo> = Vec::new();

    let bet_info_key_round = bet_info_key(round_id.u128(), &info.sender);
    let game = bet_info_storage().may_load(deps.storage, bet_info_key_round)?;
    match game {
        Some(_game) => my_game_list.push(_game),
        None => {}
    }

    for game in my_game_list {
        let round_id = game.round_id;
        let round = ROUNDS.load(deps.storage, round_id.u128())?;

        let pool_shares = round.bear_amount + round.bull_amount;
        let bet_info_key = bet_info_key(round_id.u128(), &info.sender);

        bet_info_storage().remove(deps.storage, bet_info_key.clone())?;

        let claim_info_key = claim_info_key(round_id.u128(), &info.sender);

        if round.bear_amount == Uint128::zero() || round.bull_amount == Uint128::zero() {
            winnings += game.amount;
            if game.amount > Uint128::zero() {
                claim_info_storage().save(
                    deps.storage,
                    claim_info_key,
                    &ClaimInfo {
                        player: info.sender.clone(),
                        round_id,
                        claimed_amount: winnings,
                    },
                )?;
            }
        } else {
            let round_winnings = match round.winner {
                Some(Direction::Bull) => {
                    /* Only claimable once */
                    match game.direction {
                        Direction::Bull => {
                            let won_shares = game.amount;
                            pool_shares.multiply_ratio(won_shares, round.bull_amount)
                        }
                        Direction::Bear => Uint128::zero(),
                    }
                }
                Some(Direction::Bear) => {
                    /* Only claimable once */
                    match game.direction {
                        Direction::Bull => Uint128::zero(),
                        Direction::Bear => {
                            let won_shares = game.amount;
                            pool_shares.multiply_ratio(won_shares, round.bear_amount)
                        }
                    }
                }
                None => {
                    /* Only claimable once */
                    game.amount
                }
            };

            /* Count it up */
            winnings += round_winnings;
            if round_winnings > Uint128::zero() {
                claim_info_storage().save(
                    deps.storage,
                    claim_info_key,
                    &ClaimInfo {
                        player: info.sender.clone(),
                        round_id,
                        claimed_amount: winnings,
                    },
                )?;
            }
        }
    }

    if winnings == Uint128::zero() {
        return Err(ContractError::Std(StdError::generic_err(
            "Nothing to claim",
        )));
    }

    let msg_send_winnings = BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: coins(winnings.u128(), &config.token_denom),
    };

    Ok(resp
        .add_message(msg_send_winnings)
        .add_attribute("action", "collect-winnings")
        .add_attribute("amount", winnings))
}

fn execute_bet(
    deps: DepsMut<SeiQueryWrapper>,
    info: MessageInfo,
    env: Env,
    round_id: Uint128,
    dir: Direction,
    gross: Uint128,
) -> Result<Response, ContractError> {
    assert_not_halted(deps.as_ref())?;

    let mut bet_round = assert_is_current_round(deps.as_ref(), round_id)?;
    let mut resp = Response::new();
    let config = CONFIG.load(deps.storage)?;

    let funds_sent = one_coin(&info)?;

    let totals = TOTALS_SPENT.may_load(deps.storage, info.clone().sender)?;

    if totals.is_none() {
        TOTALS_SPENT.save(deps.storage, info.clone().sender, &funds_sent.amount)?;
    } else {
        let new_totals = totals.unwrap() + funds_sent.amount;
        TOTALS_SPENT.save(deps.storage, info.clone().sender, &new_totals)?;
    }

    if funds_sent.denom != config.token_denom {
        return Err(ContractError::InvalidFunds {});
    }

    if funds_sent.amount != gross {
        return Err(ContractError::NotEnoughFunds {});
    }

    if env.block.time > bet_round.open_time {
        return Err(ContractError::Std(StdError::generic_err(format!(
            "Round {} stopped accepting bids {} second(s) ago; the next round has not yet begun",
            round_id,
            (env.block.time.seconds() - bet_round.open_time.seconds())
        ))));
    }

    let staker_fee = compute_gaming_fee(deps.as_ref(), gross)?;
    ACCUMULATED_FEE.update(deps.storage, |fee_before| -> Result<u128, StdError> {
        Ok(fee_before + staker_fee.u128())
    })?;

    /* Deduct open from the gross amount */
    let bet_amt = gross - staker_fee;

    let bet_info_key = bet_info_key(round_id.u128(), &info.sender.clone());

    let bet_info = bet_info_storage().may_load(deps.storage, bet_info_key.clone())?;

    if !bet_info.is_none() {
        return Err(ContractError::Std(StdError::generic_err(format!(
            "You have already bet for this game for {}, with amount: {}",
            bet_info.clone().unwrap().direction.to_string(),
            bet_info.unwrap().amount
        ))));
    }

    match dir {
        Direction::Bull => {
            // BULL_BETS.save(deps.storage, bet_key, &bet_amt.u128())?;
            bet_info_storage().save(
                deps.storage,
                bet_info_key.clone(),
                &BetInfo {
                    player: info.sender.clone(),
                    round_id,
                    amount: bet_amt,
                    direction: Direction::Bull,
                },
            )?;
            bet_round.bull_amount += bet_amt;
            NEXT_ROUND.save(deps.storage, &bet_round)?;
            resp = resp.add_event(Event::new("fuzio_bet").add_attributes(vec![
                ("action", "fuzio-bet".to_string()),
                ("round", round_id.to_string()),
                ("direction", "bull".to_string()),
                ("amount", bet_amt.to_string()),
                ("round_bull_total", bet_round.bull_amount.to_string()),
                ("account", info.sender.to_string()),
            ]));
        }
        Direction::Bear => {
            bet_info_storage().save(
                deps.storage,
                bet_info_key.clone(),
                &BetInfo {
                    player: info.sender.clone(),
                    round_id,
                    amount: bet_amt,
                    direction: Direction::Bear,
                },
            )?;
            bet_round.bear_amount += bet_amt;
            NEXT_ROUND.save(deps.storage, &bet_round)?;
            resp = resp.add_event(Event::new("fuzio_bet").add_attributes(vec![
                ("action", "fuzio-bet".to_string()),
                ("round", round_id.to_string()),
                ("direction", "bear".to_string()),
                ("amount", bet_amt.to_string()),
                ("round_bear_total", bet_round.bear_amount.to_string()),
                ("account", info.sender.to_string()),
            ]));
        }
    }

    Ok(resp)
}

fn execute_close_round(
    deps: DepsMut<SeiQueryWrapper>,
    env: Env,
) -> Result<Response, ContractError> {
    assert_not_halted(deps.as_ref())?;
    let now = env.block.time;
    let config = CONFIG.load(deps.storage)?;
    let mut resp: Response = Response::new();

    let collected_fee = ACCUMULATED_FEE.load(deps.storage)?;
    let mut messages = Vec::new();

    if collected_fee != 0 {
        for dev_wallet in config.clone().dev_wallet_list {
            let token_transfer_msg = BankMsg::Send {
                to_address: dev_wallet.address.to_string(),
                amount: coins(
                    (Uint128::new(collected_fee) * dev_wallet.ratio).u128(),
                    &config.token_denom,
                ),
            };
            messages.push(token_transfer_msg)
        }
    }

    let maybe_live_round = LIVE_ROUND.may_load(deps.storage)?;
    match &maybe_live_round {
        Some(live_round) => {
            if now >= live_round.close_time {
                let finished_round = compute_round_close(deps.as_ref(), live_round)?;
                ROUNDS.save(deps.storage, live_round.id.u128(), &finished_round)?;

                resp = resp.add_event(Event::new("fuzio_bet").add_attributes(vec![
                    ("round_dead", live_round.id.to_string()),
                    ("close_price", finished_round.close_price.to_string()),
                    (
                        "winner",
                        match finished_round.winner {
                            Some(w) => w.to_string(),
                            None => "everybody".to_string(),
                        },
                    ),
                ]));
                LIVE_ROUND.remove(deps.storage);
                resp = resp.add_attribute("action", "distribute_rewards");
                if collected_fee != 0 {
                    resp = resp.add_messages(messages);
                }
                ACCUMULATED_FEE.save(deps.storage, &0u128)?;
            }
        }
        None => {}
    }

    /* Close the bidding round if it is finished
     * NOTE Don't allow two live rounds at the same time - wait for the other to close
     */
    let new_bid_round = |deps: DepsMut<SeiQueryWrapper>, env: Env| -> StdResult<Uint128> {
        let id = Uint128::from(NEXT_ROUND_ID.load(deps.storage)?);
        let open_time = match LIVE_ROUND.may_load(deps.storage)? {
            Some(live_round) => live_round.close_time,
            None => env
                .block
                .time
                .plus_seconds(config.next_round_seconds.u128() as u64),
        };
        let close_time = open_time.plus_seconds(config.next_round_seconds.u128() as u64);

        NEXT_ROUND.save(
            deps.storage,
            &NextRound {
                bear_amount: Uint128::zero(),
                bull_amount: Uint128::zero(),
                bid_time: env.block.time,
                close_time,
                open_time,
                id,
            },
        )?;
        NEXT_ROUND_ID.save(deps.storage, &(id.u128() + 1u128))?;
        Ok(id)
    };

    let maybe_open_round = NEXT_ROUND.may_load(deps.storage)?;
    match &maybe_open_round {
        Some(open_round) => {
            if LIVE_ROUND.may_load(deps.storage)?.is_none() && now >= open_round.open_time {
                let live_round = compute_round_open(deps.as_ref(), env.clone(), open_round)?;
                resp = resp.add_event(Event::new("fuzio_bet").add_attributes(vec![
                    ("round_bidding_close", live_round.id.to_string()),
                    ("open_price", live_round.open_price.to_string()),
                    ("bear_amount", live_round.bear_amount.to_string()),
                    ("bull_amount", live_round.bull_amount.to_string()),
                ]));
                LIVE_ROUND.save(deps.storage, &live_round)?;
                NEXT_ROUND.remove(deps.storage);
                let new_round_id = new_bid_round(deps, env)?;
                resp = resp.add_event(
                    Event::new("fuzio_bet").add_attribute("round_bidding_open", new_round_id),
                );
            }
        }
        None => {
            let new_round_id = new_bid_round(deps, env)?;
            resp = resp.add_event(
                Event::new("fuzio_bet").add_attribute("round_bidding_open", new_round_id),
            );
        }
    }

    Ok(resp.add_attribute("action", "close-round"))
}

fn execute_update_config(
    deps: DepsMut<SeiQueryWrapper>,
    info: MessageInfo,
    u_config: Config,
) -> Result<Response, ContractError> {
    assert_is_admin(deps.as_ref(), info)?;

    CONFIG.save(deps.storage, &u_config)?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<SeiQueryWrapper>, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::Status {} => to_binary(&query_status(deps, env)?),
        QueryMsg::MyCurrentPosition { address } => {
            to_binary(&query_my_current_position(deps, address)?)
        }
        QueryMsg::FinishedRound { round_id } => to_binary(&query_finished_round(deps, round_id)?),
        QueryMsg::MyGameList {
            player,
            start_after,
            limit,
        } => to_binary(&query_my_games(deps, player, start_after, limit)?),
        QueryMsg::MyPendingReward { player } => to_binary(&query_my_pending_reward(deps, player)?),
        QueryMsg::MyPendingRewardRounds { player } => {
            to_binary(&query_my_pending_reward_rounds(deps, player)?)
        }
        QueryMsg::GetUsersPerRound {
            round_id,
            start_after,
            limit,
        } => to_binary(&query_users_per_round(deps, round_id, start_after, limit)?),
        QueryMsg::MyPendingRewardRound { round_id, player } => {
            to_binary(&query_my_pending_reward_round(deps, round_id, player)?)
        }
        QueryMsg::GetClaimInfoPerRound {
            round_id,
            start_after,
            limit,
        } => to_binary(&query_claim_info_per_round(
            deps,
            round_id,
            start_after,
            limit,
        )?),
        QueryMsg::GetClaimInfoByUser {
            player,
            start_after,
            limit,
        } => to_binary(&query_claim_info_by_user(deps, player, start_after, limit)?),
        QueryMsg::TotalSpent { player } => to_binary(&query_total_spent(deps, player)?),
        QueryMsg::GetAdmins {} => to_binary(&query_get_admins(deps)?),
    }
}

fn query_finished_round(
    deps: Deps<SeiQueryWrapper>,
    round_id: Uint128,
) -> StdResult<FinishedRound> {
    let round = ROUNDS.load(deps.storage, round_id.u128())?;
    Ok(round)
}

fn query_my_current_position(
    deps: Deps<SeiQueryWrapper>,
    address: String,
) -> StdResult<MyCurrentPositionResponse> {
    let round_id = NEXT_ROUND_ID.load(deps.storage)?;
    let next_bet_key = (round_id - 1, deps.api.addr_validate(&address)?);

    let next_bet_info = bet_info_storage().may_load(deps.storage, next_bet_key)?;

    let mut next_bull_amount = Uint128::zero();
    let mut next_bear_amount = Uint128::zero();

    match next_bet_info {
        Some(bet_info) => match bet_info.direction {
            Direction::Bull => {
                next_bull_amount = bet_info.amount;
            }
            Direction::Bear => {
                next_bear_amount = bet_info.amount;
            }
        },
        None => {}
    }

    let mut live_bull_amount: Uint128 = Uint128::zero();
    let mut live_bear_amount: Uint128 = Uint128::zero();
    if round_id > 1 {
        let live_bet_key = (round_id - 2, deps.api.addr_validate(&address)?);
        let live_bet_info = bet_info_storage().may_load(deps.storage, live_bet_key)?;
        match live_bet_info {
            Some(bet_info) => match bet_info.direction {
                Direction::Bull => {
                    live_bull_amount = bet_info.amount;
                }
                Direction::Bear => {
                    live_bear_amount = bet_info.amount;
                }
            },
            None => {}
        }
    }

    Ok(MyCurrentPositionResponse {
        next_bear_amount,
        next_bull_amount,
        live_bear_amount,
        live_bull_amount,
    })
}

fn query_status(deps: Deps<SeiQueryWrapper>, env: Env) -> StdResult<StatusResponse> {
    let live_round = LIVE_ROUND.may_load(deps.storage)?;
    let bidding_round = NEXT_ROUND.may_load(deps.storage)?;
    let current_time = env.block.time;

    Ok(StatusResponse {
        bidding_round,
        live_round,
        current_time,
    })
}

fn query_config(deps: Deps<SeiQueryWrapper>) -> StdResult<ConfigResponse> {
    CONFIG.load(deps.storage)
}

pub fn query_my_games(
    deps: Deps<SeiQueryWrapper>,
    player: Addr,
    start_after: Option<Uint128>,
    limit: Option<u32>,
) -> StdResult<MyGameResponse> {
    let limit = limit.unwrap_or(DEFAULT_QUERY_LIMIT).min(MAX_QUERY_LIMIT) as usize;

    let start = if let Some(start) = start_after {
        let round_id = start;
        Some(Bound::exclusive(bet_info_key(round_id.u128(), &player)))
    } else {
        None
    };

    let my_game_list = bet_info_storage()
        .idx
        .player
        .prefix(player.clone())
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|res| res.map(|item| item.1))
        .collect::<StdResult<Vec<_>>>()?;
    Ok(MyGameResponse { my_game_list })
}

//it is used for backend saving
pub fn query_users_per_round(
    deps: Deps<SeiQueryWrapper>,
    round_id: Uint128,
    start_after: Option<Addr>,
    limit: Option<u32>,
) -> StdResult<RoundUsersResponse> {
    let limit = limit.unwrap_or(DEFAULT_QUERY_LIMIT).min(MAX_QUERY_LIMIT) as usize;

    let start = if let Some(start) = start_after {
        let player = start;
        Some(Bound::exclusive(bet_info_key(round_id.u128(), &player)))
    } else {
        None
    };

    let round_users = bet_info_storage()
        .idx
        .round_id
        .prefix(round_id.u128())
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|res| res.map(|item| item.1))
        .collect::<StdResult<Vec<_>>>()?;

    Ok(RoundUsersResponse { round_users })
}

pub fn query_claim_info_per_round(
    deps: Deps<SeiQueryWrapper>,
    round_id: Uint128,
    start_after: Option<Addr>,
    limit: Option<u32>,
) -> StdResult<ClaimInfoResponse> {
    let limit = limit.unwrap_or(DEFAULT_QUERY_LIMIT).min(MAX_QUERY_LIMIT) as usize;

    let start = if let Some(start) = start_after {
        let player = start;
        Some(Bound::exclusive(bet_info_key(round_id.u128(), &player)))
    } else {
        None
    };

    let claim_info = claim_info_storage()
        .idx
        .round_id
        .prefix(round_id.u128())
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|res| res.map(|item| item.1))
        .collect::<StdResult<Vec<_>>>()?;

    Ok(ClaimInfoResponse { claim_info })
}

pub fn query_claim_info_by_user(
    deps: Deps<SeiQueryWrapper>,
    player: Addr,
    start_after: Option<Uint128>,
    limit: Option<u32>,
) -> StdResult<ClaimInfoResponse> {
    let limit = limit.unwrap_or(DEFAULT_QUERY_LIMIT).min(MAX_QUERY_LIMIT) as usize;

    let start = if let Some(start) = start_after {
        let round_id = start;
        Some(Bound::exclusive(bet_info_key(round_id.u128(), &player)))
    } else {
        None
    };

    let claim_info = claim_info_storage()
        .idx
        .player
        .prefix(player)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|res| res.map(|item| item.1))
        .collect::<StdResult<Vec<_>>>()?;

    Ok(ClaimInfoResponse { claim_info })
}

pub fn query_my_pending_reward(
    deps: Deps<SeiQueryWrapper>,
    player: Addr,
) -> StdResult<PendingRewardResponse> {
    let my_game_list = query_my_games_without_limit(deps, player.clone())?;
    let mut winnings = Uint128::zero();

    for game in my_game_list.my_game_list {
        let round_id = game.round_id;
        let round = ROUNDS.may_load(deps.storage, round_id.u128())?;

        if round.is_none() {
            continue;
        }
        let round = round.unwrap();

        let pool_shares = round.bear_amount + round.bull_amount;

        if round.bear_amount == Uint128::zero() || round.bull_amount == Uint128::zero() {
            winnings += game.amount;
        } else {
            let round_winnings = match round.winner {
                Some(Direction::Bull) => {
                    /* Only claimable once */
                    match game.direction {
                        Direction::Bull => {
                            let won_shares = game.amount;
                            pool_shares.multiply_ratio(won_shares, round.bull_amount)
                        }
                        Direction::Bear => Uint128::zero(),
                    }
                }
                Some(Direction::Bear) => {
                    /* Only claimable once */
                    match game.direction {
                        Direction::Bull => Uint128::zero(),
                        Direction::Bear => {
                            let won_shares = game.amount;
                            pool_shares.multiply_ratio(won_shares, round.bear_amount)
                        }
                    }
                }
                None => {
                    /* Only claimable once */
                    game.amount
                }
            };

            /* Count it up */
            winnings += round_winnings;
        }
    }

    Ok(PendingRewardResponse {
        pending_reward: winnings,
    })
}

pub fn query_my_pending_reward_rounds(
    deps: Deps<SeiQueryWrapper>,
    player: Addr,
) -> StdResult<PendingRewardRoundsResponse> {
    let my_game_list = query_my_games_without_limit(deps, player.clone())?;
    let mut winnings = Uint128::zero();
    let mut winnings_per_round: Vec<(Uint128, Uint128)> = vec![];

    for game in my_game_list.my_game_list {
        let round_id = game.round_id;
        let round = ROUNDS.may_load(deps.storage, round_id.u128())?;

        if round.is_none() {
            continue;
        }
        let round = round.unwrap();

        let pool_shares = round.bear_amount + round.bull_amount;

        if round.bear_amount == Uint128::zero() || round.bull_amount == Uint128::zero() {
            winnings += game.amount;
            winnings_per_round.push((round_id, game.amount));
        } else {
            let round_winnings = match round.winner {
                Some(Direction::Bull) => {
                    /* Only claimable once */
                    match game.direction {
                        Direction::Bull => {
                            let won_shares = game.amount;
                            pool_shares.multiply_ratio(won_shares, round.bull_amount)
                        }
                        Direction::Bear => Uint128::zero(),
                    }
                }
                Some(Direction::Bear) => {
                    /* Only claimable once */
                    match game.direction {
                        Direction::Bull => Uint128::zero(),
                        Direction::Bear => {
                            let won_shares = game.amount;
                            pool_shares.multiply_ratio(won_shares, round.bear_amount)
                        }
                    }
                }
                None => {
                    /* Only claimable once */
                    game.amount
                }
            };

            /* Count it up */
            winnings += round_winnings;
            if round_winnings != Uint128::zero() {
                winnings_per_round.push((round_id, round_winnings))
            }
        }
    }

    Ok(PendingRewardRoundsResponse {
        pending_reward_rounds: winnings_per_round,
        pending_reward_total: winnings,
    })
}

pub fn query_my_pending_reward_round(
    deps: Deps<SeiQueryWrapper>,
    round_id: Uint128,
    player: Addr,
) -> StdResult<PendingRewardResponse> {
    let mut winnings = Uint128::zero();
    let mut my_game_list: Vec<BetInfo> = Vec::new();

    let bet_info_key = bet_info_key(round_id.u128(), &player);
    let game = bet_info_storage().may_load(deps.storage, bet_info_key)?;
    match game {
        Some(_game) => my_game_list.push(_game),
        None => {}
    }

    for game in my_game_list {
        let round_id = game.round_id;
        let round = ROUNDS.may_load(deps.storage, round_id.u128())?;

        if round.is_none() {
            continue;
        }
        let round = round.unwrap();

        let pool_shares = round.bear_amount + round.bull_amount;

        if round.bear_amount == Uint128::zero() || round.bull_amount == Uint128::zero() {
            winnings += game.amount;
        } else {
            let round_winnings = match round.winner {
                Some(Direction::Bull) => {
                    /* Only claimable once */
                    match game.direction {
                        Direction::Bull => {
                            let won_shares = game.amount;
                            pool_shares.multiply_ratio(won_shares, round.bull_amount)
                        }
                        Direction::Bear => Uint128::zero(),
                    }
                }
                Some(Direction::Bear) => {
                    /* Only claimable once */
                    match game.direction {
                        Direction::Bull => Uint128::zero(),
                        Direction::Bear => {
                            let won_shares = game.amount;
                            pool_shares.multiply_ratio(won_shares, round.bear_amount)
                        }
                    }
                }
                None => {
                    /* Only claimable once */
                    game.amount
                }
            };

            /* Count it up */
            winnings += round_winnings;
        }
    }

    Ok(PendingRewardResponse {
        pending_reward: winnings,
    })
}

pub fn query_my_games_without_limit(
    deps: Deps<SeiQueryWrapper>,
    player: Addr,
) -> StdResult<MyGameResponse> {
    let my_game_list = bet_info_storage()
        .idx
        .player
        .prefix(player.clone())
        .range(deps.storage, None, None, Order::Ascending)
        .map(|res| res.map(|item| item.1))
        .collect::<StdResult<Vec<_>>>()?;
    Ok(MyGameResponse { my_game_list })
}

pub fn query_total_spent(
    deps: Deps<SeiQueryWrapper>,
    player: Addr,
) -> StdResult<TotalSpentResponse> {
    let total = TOTALS_SPENT.may_load(deps.storage, player)?;

    Ok(TotalSpentResponse {
        total_spent: total.unwrap_or(Uint128::zero()),
    })
}

pub fn query_get_admins(deps: Deps<SeiQueryWrapper>) -> StdResult<AdminsResponse> {
    let admins = ADMINS.load(deps.storage)?;

    Ok(AdminsResponse { admins })
}

fn assert_is_current_round(deps: Deps<SeiQueryWrapper>, round_id: Uint128) -> StdResult<NextRound> {
    let open_round = NEXT_ROUND.load(deps.storage)?;

    if round_id != open_round.id {
        return Err(StdError::generic_err(format!(
            "Tried to open at round {} but it's currently round {}",
            round_id, open_round.id
        )));
    }

    Ok(open_round)
}

fn compute_gaming_fee(deps: Deps<SeiQueryWrapper>, gross: Uint128) -> StdResult<Uint128> {
    let staker_fee = CONFIG.load(deps.storage)?.gaming_fee;

    staker_fee
        .checked_multiply_ratio(gross, FEE_PRECISION * 100)
        .map_err(|e| StdError::generic_err(e.to_string()))
}

fn compute_round_open(
    deps: Deps<SeiQueryWrapper>,
    env: Env,
    round: &NextRound,
) -> Result<LiveRound, ContractError> {
    /* TODO */
    let open_price = get_current_price(deps)?;
    let config = CONFIG.load(deps.storage)?;

    Ok(LiveRound {
        id: round.id,
        bid_time: round.bid_time,
        open_time: env.block.time,
        close_time: env
            .block
            .time
            .plus_seconds(config.next_round_seconds.u128() as u64),
        open_price,
        bull_amount: round.bull_amount,
        bear_amount: round.bear_amount,
    })
}

fn get_current_price(deps: Deps<SeiQueryWrapper>) -> Result<Decimal, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let querier = SeiQuerier::new(&deps.querier);
    let res: ExchangeRatesResponse = querier.query_exchange_rates()?;

    let exchange_rate = res
        .denom_oracle_exchange_rate_pairs
        .iter()
        .find(|rate| config.bet_token_denom == rate.denom);

    if exchange_rate.is_none() {
        return Err(ContractError::PriceNotFoundInOracle {});
    }

    Ok(exchange_rate.unwrap().oracle_exchange_rate.exchange_rate)
}

fn compute_round_close(
    deps: Deps<SeiQueryWrapper>,
    round: &LiveRound,
) -> Result<FinishedRound, ContractError> {
    let close_price = get_current_price(deps)?;

    let winner = match close_price.cmp(&round.open_price) {
        std::cmp::Ordering::Greater =>
        /* Bulls win */
        {
            Some(Direction::Bull)
        }
        std::cmp::Ordering::Less =>
        /* Bears win */
        {
            Some(Direction::Bear)
        }
        std::cmp::Ordering::Equal =>
        /* Weird case where nobody was right */
        {
            None
        }
    };

    Ok(FinishedRound {
        id: round.id,
        bid_time: round.bid_time,
        open_time: round.open_time,
        close_time: round.close_time,
        open_price: round.open_price,
        bear_amount: round.bear_amount,
        bull_amount: round.bull_amount,
        winner,
        close_price,
    })
}

fn assert_not_halted(deps: Deps<SeiQueryWrapper>) -> StdResult<bool> {
    let is_halted = IS_HALTED.load(deps.storage)?;
    if is_halted {
        return Err(StdError::generic_err("Contract is halted"));
    }
    Ok(true)
}

fn execute_update_halt(
    deps: DepsMut<SeiQueryWrapper>,
    info: MessageInfo,
    is_halted: bool,
) -> Result<Response, ContractError> {
    assert_is_admin(deps.as_ref(), info)?;
    IS_HALTED.save(deps.storage, &is_halted)?;
    Ok(Response::new().add_event(Event::new("fuzio_beta").add_attribute("halt_games", "true")))
}

fn assert_is_admin(deps: Deps<SeiQueryWrapper>, info: MessageInfo) -> StdResult<bool> {
    let admins = ADMINS.load(deps.storage)?;
    if !admins.contains(&info.sender) {
        return Err(StdError::generic_err(format!(
            "Only an admin can execute this function. Sender: {}",
            info.sender
        )));
    }

    Ok(true)
}

fn execute_add_admin(
    deps: DepsMut<SeiQueryWrapper>,
    info: MessageInfo,
    new_admin: Addr,
) -> Result<Response, ContractError> {
    assert_is_admin(deps.as_ref(), info)?;
    deps.api.addr_validate(&new_admin.to_string())?;
    let mut admins = ADMINS.load(deps.storage)?;

    admins.push(new_admin.clone());

    ADMINS.save(deps.storage, &admins)?;

    Ok(Response::new().add_attribute("add_admin", new_admin.to_string()))
}

fn execute_remove_admin(
    deps: DepsMut<SeiQueryWrapper>,
    info: MessageInfo,
    old_admin: Addr,
) -> Result<Response, ContractError> {
    assert_is_admin(deps.as_ref(), info)?;
    let mut admins = ADMINS.load(deps.storage)?;
    admins.retain(|admin| admin != old_admin);

    if admins.is_empty() {
        return Err(ContractError::NeedOneAdmin {});
    }

    ADMINS.save(deps.storage, &admins)?;
    Ok(Response::new().add_attribute("remove_admin", old_admin.to_string()))
}

fn execute_modify_dev_wallets(
    deps: DepsMut<SeiQueryWrapper>,
    info: MessageInfo,
    new_wallets: Vec<WalletInfo>,
) -> Result<Response, ContractError> {
    assert_is_admin(deps.as_ref(), info)?;
    let mut total_ratio = Decimal::zero();
    for dev_wallet in new_wallets.clone() {
        total_ratio = total_ratio + dev_wallet.ratio;
    }

    if total_ratio != Decimal::one() {
        return Err(ContractError::WrongRatio {});
    }

    let mut config = CONFIG.load(deps.storage)?;
    config.dev_wallet_list = new_wallets;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "new_dev_wallets"))
}
