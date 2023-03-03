use cosmwasm_std::testing::{
    mock_dependencies, mock_dependencies_with_balances, mock_env, mock_info,
};
use cosmwasm_std::{coin, Addr, Uint128};

use crate::state::BONDING_ASSETS_LIMIT;
use crate::ContractError;
use white_whale::whale_lair::{AssetInfo, Config};

use crate::tests::robot::TestingRobot;

#[test]
fn test_instantiate_successfully() {
    let mut robot = TestingRobot::default();

    robot
        .instantiate(
            1_000u64,
            1u8,
            vec![
                AssetInfo::NativeToken {
                    denom: "ampWHALE".to_string(),
                },
                AssetInfo::NativeToken {
                    denom: "bWHALE".to_string(),
                },
            ],
            &vec![],
        )
        .assert_config(Config {
            owner: Addr::unchecked("owner"),
            unbonding_period: 1_000u64,
            growth_rate: 1u8,
            bonding_assets: vec![
                AssetInfo::NativeToken {
                    denom: "ampWHALE".to_string(),
                },
                AssetInfo::NativeToken {
                    denom: "bWHALE".to_string(),
                },
            ],
        });
}

#[test]
fn test_instantiate_unsuccessfully() {
    let mut robot = TestingRobot::default();

    // over bonding assets limit
    robot.instantiate_err(
        1_000u64,
        1u8,
        vec![
            AssetInfo::NativeToken {
                denom: "ampWHALE".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "bWHALE".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uwhale".to_string(),
            },
        ],
        &vec![],
        |error| {
            println!("1 --{:?}", error);
            println!("2 --{:?}", error.root_cause());
            //println!("3 --{:?}", error.root_cause().downcast_ref::<ContractError>());
            // assert_eq!(
            //     error.root_cause().downcast_ref::<ContractError>().unwrap(),
            //     &ContractError::InvalidBondingAssetsLimit(BONDING_ASSETS_LIMIT, 3));
        },
    );

    // invalid tokens
    robot.instantiate_err(
        1_000u64,
        1u8,
        vec![AssetInfo::Token {
            contract_addr: "contract123".to_string(),
        }],
        &vec![],
        |error| {
            println!("1 --{:?}", error);
            println!("2 --{:?}", error.root_cause());
            //println!("3 --{:?}", error.root_cause().downcast_ref::<ContractError>());

            // assert_eq!(
            //    error.root_cause().downcast_mut::<ContractError>().unwrap(),
            //    ContractError::InvalidBondingAsset {});
        },
    );
}
