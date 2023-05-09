use cosmwasm_std::{Addr, Deps, Order, StdResult, Uint64};
use cw_storage_plus::{Item, Map};

use white_whale::fee_distributor::{
    ClaimableEpochsResponse, Config, Epoch, EpochResponse, LastClaimedEpochResponse,
};

pub const CONFIG: Item<Config> = Item::new("config");
pub const LAST_CLAIMED_EPOCH: Map<&Addr, Uint64> = Map::new("last_claimed_epoch");
pub const EPOCHS: Map<&[u8], Epoch> = Map::new("epochs");

/// Returns the current epoch, which is the last on the EPOCHS map.
pub fn get_current_epoch(deps: Deps) -> StdResult<EpochResponse> {
    let option = EPOCHS
        .range(deps.storage, None, None, Order::Descending)
        .next();

    let epoch = match option {
        Some(Ok((_, epoch))) => epoch,
        _ => Epoch::default(),
    };

    Ok(EpochResponse { epoch })
}

/// Returns the [Epoch] with the given id.
pub fn get_epoch(deps: Deps, id: Uint64) -> StdResult<EpochResponse> {
    let option = EPOCHS.may_load(deps.storage, &id.to_be_bytes())?;

    let epoch = match option {
        Some(epoch) => epoch,
        None => Epoch::default(),
    };

    Ok(EpochResponse { epoch })
}

/// Returns the epoch that is falling out the grace period, which is the one expiring after creating
/// a new epoch is created.
pub fn get_expiring_epoch(deps: Deps) -> StdResult<Option<Epoch>> {
    let grace_period = CONFIG.load(deps.storage)?.grace_period;

    // last epochs within the grace period + 1
    let epochs = EPOCHS
        .range(deps.storage, None, None, Order::Descending)
        .take(grace_period.u64() as usize)
        .map(|item| {
            let (_, epoch) = item?;
            Ok(epoch)
        })
        .collect::<StdResult<Vec<Epoch>>>()?;

    println!("epochs: {:?}", epochs);
    println!("grace_period: {:?}", grace_period);

    // it means there is one epoch that is falling out of the grace period once the new one is created
    // i.e. the last epoch in the vector
    if epochs.len() == grace_period.u64() as usize {
        // return the last epoch, which is in the grace_period + 1 index
        // Ok(Some(epochs[grace_period.u64() as usize].clone()))
        Ok(Some(epochs.last().cloned().unwrap_or_default()))
    } else {
        // nothing is expiring yet
        Ok(None)
    }
}

/// Returns the epochs that are within the grace period, i.e. the ones which fees can still be claimed.
/// The result is ordered by epoch id, descending. Thus, the first element is the current epoch.
pub fn get_claimable_epochs(deps: Deps) -> StdResult<ClaimableEpochsResponse> {
    let grace_period = CONFIG.load(deps.storage)?.grace_period;

    let epochs = EPOCHS
        .range(deps.storage, None, None, Order::Descending)
        .take(grace_period.u64() as usize)
        .map(|item| {
            let (_, epoch) = item?;
            Ok(epoch)
        })
        .collect::<StdResult<Vec<Epoch>>>()?;

    Ok(ClaimableEpochsResponse { epochs })
}

/// Returns the epochs that can be claimed by the given address.
pub fn query_claimable(deps: Deps, address: &Addr) -> StdResult<ClaimableEpochsResponse> {
    let mut claimable_epochs = get_claimable_epochs(deps)?.epochs;
    let last_claimed_epoch = LAST_CLAIMED_EPOCH.may_load(deps.storage, address)?;

    // filter out epochs that have already been claimed by the user
    if let Some(last_claimed_epoch) = last_claimed_epoch {
        claimable_epochs.retain(|epoch| epoch.id > last_claimed_epoch);
    } else {
        // if there is no last claimed epoch, the user has not even bonded, as that is set when the user bonds
        // for the first time
        return Ok(ClaimableEpochsResponse { epochs: vec![] });
    };

    // filter out epochs that have no available fees. This would only happen in case the grace period
    // gets increased after epochs have expired, which would lead to make them available for claiming
    // again without any available rewards, as those were forwarded to newer epochs.
    claimable_epochs.retain(|epoch| !epoch.available.is_empty());

    Ok(ClaimableEpochsResponse {
        epochs: claimable_epochs,
    })
}

/// Returns the last epoch that was claimed by the given address. Returns the default epoch if
/// the address has not claimed any epoch yet.
pub fn get_last_claimed_epoch(deps: Deps, address: &Addr) -> StdResult<LastClaimedEpochResponse> {
    let last_claimed_epoch = LAST_CLAIMED_EPOCH.may_load(deps.storage, address)?;

    if let Some(last_claimed_epoch) = last_claimed_epoch {
        return Ok(LastClaimedEpochResponse {
            address: address.clone(),
            last_claimed_epoch_id: last_claimed_epoch,
        });
    };

    Ok(LastClaimedEpochResponse {
        address: address.clone(),
        last_claimed_epoch_id: Uint64::zero(),
    })
}
