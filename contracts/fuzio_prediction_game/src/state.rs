use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Index, IndexList, IndexedMap, Item, Map, MultiIndex};
use fuzio_bet::fuzio_prediction_game::{BetInfoKey, BetInfo, ClaimInfoKey, ClaimInfo};
use fuzio_bet::fuzio_prediction_game::{Config, FinishedRound, LiveRound, NextRound};

pub const IS_HALTED: Item<bool> = Item::new("is_halted");
pub const CONFIG: Item<Config> = Item::new("config");
pub const NEXT_ROUND_ID: Item<u128> = Item::new("next_round_id");
/* The round that's open for betting */
pub const NEXT_ROUND: Item<NextRound> = Item::new("next_round");
/* The live round; not accepting bets */
pub const LIVE_ROUND: Item<LiveRound> = Item::new("live_round");

pub const ROUNDS: Map<u128, FinishedRound> = Map::new("rounds");

pub const ADMINS: Item<Vec<Addr>> = Item::new("admins");

pub const TOTALS_SPENT: Map<Addr, Uint128> = Map::new("amounts_spent");

/// Convenience bid key constructor
pub fn bet_info_key(round_id: u128, player: &Addr) -> BetInfoKey {
    (round_id, player.clone())
}

/// Defines incides for accessing bids
pub struct BetInfoIndicies<'a> {
    pub player: MultiIndex<'a, Addr, BetInfo, BetInfoKey>,
    pub round_id: MultiIndex<'a, u128, BetInfo, BetInfoKey>,
}

impl<'a> IndexList<BetInfo> for BetInfoIndicies<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<BetInfo>> + '_> {
        let v: Vec<&dyn Index<BetInfo>> = vec![&self.player, &self.round_id];
        Box::new(v.into_iter())
    }
}

pub fn bet_info_storage<'a>() -> IndexedMap<'a, BetInfoKey, BetInfo, BetInfoIndicies<'a>> {
    let indexes = BetInfoIndicies {
        player: MultiIndex::new(
            |_pk: &[u8], d: &BetInfo| d.player.clone(),
            "bet_info",
            "bet_info_collection",
        ),
        round_id: MultiIndex::new(
            |_pk: &[u8], d: &BetInfo| d.round_id.u128(),
            "bet_info",
            "round_id",
        ),
    };
    IndexedMap::new("bet_info", indexes)
}
/// Convenience bid key constructor
pub fn claim_info_key(round_id: u128, player: &Addr) -> ClaimInfoKey {
    (round_id, player.clone())
}

/// Defines incides for accessing bids
pub struct ClaimInfoIndicies<'a> {
    pub player: MultiIndex<'a, Addr, ClaimInfo, ClaimInfoKey>,
    pub round_id: MultiIndex<'a, u128, ClaimInfo, ClaimInfoKey>,
}

impl<'a> IndexList<ClaimInfo> for ClaimInfoIndicies<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<ClaimInfo>> + '_> {
        let v: Vec<&dyn Index<ClaimInfo>> = vec![&self.player, &self.round_id];
        Box::new(v.into_iter())
    }
}

pub fn claim_info_storage<'a>() -> IndexedMap<'a, ClaimInfoKey, ClaimInfo, ClaimInfoIndicies<'a>> {
    let indexes = ClaimInfoIndicies {
        player: MultiIndex::new(
            |_pk: &[u8], d: &ClaimInfo| d.player.clone(),
            "claim_info",
            "claim_info_collection",
        ),
        round_id: MultiIndex::new(
            |_pk: &[u8], d: &ClaimInfo| d.round_id.u128(),
            "claim_info",
            "claim_round_id",
        ),
    };
    IndexedMap::new("claim_info", indexes)
}
