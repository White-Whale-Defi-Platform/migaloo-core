use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{Addr, Timestamp, Uint64};

use white_whale::fee_distributor::{Epoch, EpochConfig};
use white_whale::pool_network::asset::AssetInfo;

use crate::tests::robot::TestingRobot;
use crate::tests::test_helpers;
use crate::ContractError;

#[test]
fn test_current_epoch_no_epochs() {
    let mut robot = TestingRobot::new(mock_dependencies(), mock_env());

    robot
        .instantiate_default()
        .assert_current_epoch(&Epoch::default())
        .query_epoch(Uint64::new(10), |res| {
            // epoch 10 doesn't exist, it should return the default value
            let (_, epoch) = res.unwrap();
            assert_eq!(epoch, Epoch::default());
        });
}

#[test]
fn test_expiring_epoch() {
    let mut robot = TestingRobot::new(mock_dependencies(), mock_env());
    let epochs = test_helpers::get_epochs();

    robot
        .instantiate_default()
        .add_epochs_to_state(epochs.clone())
        .assert_expiring_epoch(Some(&epochs[1]));
}

#[test]
fn test_create_genesis_epoch() {
    let mut robot = TestingRobot::new(mock_dependencies(), mock_env());
    let grace_period = Uint64::new(2);
    let distribution_asset = AssetInfo::NativeToken {
        denom: "uwhale".to_string(),
    };
    let epoch_config = EpochConfig {
        duration: Uint64::new(86_400_000_000_000u64), // a day
        genesis_epoch: Uint64::new(1678802400_000000000u64), // March 14, 2023 2:00:00 PM
    };

    robot.env.block.time = Timestamp::from_nanos(1678802300_000000000u64); // before genesis epoch

    robot
        .instantiate(
            mock_info("owner", &[]),
            "bonding_contract_addr".to_string(),
            "fee_collector_addr".to_string(),
            grace_period,
            epoch_config.clone(),
            distribution_asset.clone(),
        )
        .create_new_epoch(mock_info("owner", &[]), |res| {
            let err = res.unwrap_err();
            assert_eq!(err, ContractError::GenesisEpochNotStarted {});
        });

    // set the time at genesis epoch
    robot.env.block.time = Timestamp::from_nanos(1678802400_000000000u64); // before genesis epoch

    robot.create_new_epoch(mock_info("owner", &[]), |res| {
        // all good now
        res.unwrap();
    });
}

#[test]
fn test_set_last_claimed_epoch() {
    let mut robot = TestingRobot::new(mock_dependencies(), mock_env());

    robot
        .instantiate_default()
        .query_last_claimed_epoch(Addr::unchecked("bob".to_string()), |res| {
            let (_, res) = res.unwrap();
            // bob has claimed anything
            assert_eq!(res.last_claimed_epoch_id, Uint64::zero());
        })
        .set_last_claimed_epoch(
            mock_info("bob", &[]),
            Addr::unchecked("bob".to_string()),
            Uint64::new(5u64),
            |res| {
                res.unwrap();
            },
        )
        .query_last_claimed_epoch(Addr::unchecked("bob".to_string()), |res| {
            let (_, res) = res.unwrap();
            // bob has claimed anything
            assert_eq!(res.last_claimed_epoch_id, Uint64::new(5u64));
        });
}
