use cosmwasm_std::{to_binary, CosmosMsg, DepsMut, Env, MessageInfo, Response, Uint128, WasmMsg};
use cw20::{AllowanceResponse, Cw20ExecuteMsg};
use terraswap::{asset::AssetInfo, querier::query_token_info};

use crate::{
    error::{StdResult, VaultError},
    state::{COLLECTED_PROTOCOL_FEES, CONFIG, LOAN_COUNTER},
};

pub fn deposit(deps: DepsMut, env: Env, info: MessageInfo, amount: Uint128) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;

    // check that deposits are enabled
    if !config.deposit_enabled {
        return Err(VaultError::DepositsDisabled {});
    }

    // check that we are not currently in a flash-loan
    if LOAN_COUNTER.load(deps.storage)? != 0 {
        // more than 0 loans is being performed currently
        return Err(VaultError::DepositDuringLoan {});
    }

    // check that user sent assets they said they did
    let sent_funds = match config.asset_info.clone() {
        AssetInfo::NativeToken { denom } => info
            .funds
            .iter()
            .filter(|c| c.denom == denom)
            .map(|c| c.amount)
            .sum::<Uint128>(),
        AssetInfo::Token { contract_addr } => {
            let allowance: AllowanceResponse = deps.querier.query_wasm_smart(
                contract_addr,
                &cw20::Cw20QueryMsg::Allowance {
                    owner: info.sender.clone().into_string(),
                    spender: env.contract.address.clone().into_string(),
                },
            )?;

            allowance.allowance
        }
    };
    if sent_funds != amount {
        return Err(VaultError::FundsMismatch {
            sent: sent_funds,
            wanted: amount,
        });
    }

    let mut messages: Vec<CosmosMsg> = vec![];
    // add cw20 transfer message if needed
    if let AssetInfo::Token { contract_addr } = config.asset_info.clone() {
        messages.push(
            WasmMsg::Execute {
                contract_addr,
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.clone().into_string(),
                    recipient: env.contract.address.clone().into_string(),
                    amount,
                })?,
                funds: vec![],
            }
            .into(),
        )
    }

    // mint LP token for the sender
    let total_lp_share =
        query_token_info(&deps.querier, config.liquidity_token.clone())?.total_supply;

    let lp_amount = if total_lp_share.is_zero() {
        // first depositor to the vault, mint LP tokens 1:1
        amount
    } else {
        // return based on a share of the total pool
        let collected_protocol_fees = COLLECTED_PROTOCOL_FEES.load(deps.storage)?;
        let total_deposits = config
            .asset_info
            .query_pool(&deps.querier, deps.api, env.contract.address)?
            .checked_sub(collected_protocol_fees.amount)?;

        amount
            .checked_mul(total_lp_share)?
            .checked_div(total_deposits)?
    };

    messages.push(
        WasmMsg::Execute {
            contract_addr: config.liquidity_token.into_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                recipient: info.sender.into_string(),
                amount: lp_amount,
            })?,
            funds: vec![],
        }
        .into(),
    );

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(vec![("method", "deposit"), ("amount", &amount.to_string())]))
}

#[cfg(test)]
mod test {
    use cosmwasm_std::{
        coins,
        testing::{mock_dependencies, mock_env, mock_info},
        to_binary, Addr, Response, Uint128, WasmMsg,
    };
    use cw20::Cw20ExecuteMsg;
    use terraswap::asset::{Asset, AssetInfo};
    use vault_network::vault::Config;

    use crate::{
        contract::execute,
        error::VaultError,
        execute::deposit,
        state::{COLLECTED_PROTOCOL_FEES, CONFIG, LOAN_COUNTER},
        tests::{get_fees, mock_creator, mock_dependencies_lp, mock_execute},
    };

    #[test]
    fn can_deposit_native() {
        let env = mock_env();
        let mut deps = mock_dependencies_lp(
            &[],
            &[],
            vec![(
                "creator".to_string(),
                env.contract.address.clone().into_string(),
                Uint128::new(5_000),
            )],
        );

        // inject lp token address to config
        CONFIG
            .save(
                &mut deps.storage,
                &Config {
                    owner: mock_creator().sender,
                    liquidity_token: Addr::unchecked("lp_token"),
                    asset_info: AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    deposit_enabled: true,
                    flash_loan_enabled: true,
                    withdraw_enabled: true,
                    fee_collector_addr: Addr::unchecked("fee_collector"),
                    fees: get_fees(),
                },
            )
            .unwrap();

        // inject loan counter
        LOAN_COUNTER.save(&mut deps.storage, &0).unwrap();

        let res = execute(
            deps.as_mut(),
            env,
            mock_info("creator", &coins(5_000, "uluna")),
            vault_network::vault::ExecuteMsg::Deposit {
                amount: Uint128::new(5_000),
            },
        );

        assert_eq!(
            res.unwrap(),
            Response::new()
                .add_attributes(vec![("method", "deposit"), ("amount", "5000")])
                .add_message(WasmMsg::Execute {
                    contract_addr: "lp_token".to_string(),
                    funds: vec![],
                    msg: to_binary(&Cw20ExecuteMsg::Mint {
                        recipient: "creator".to_string(),
                        amount: Uint128::new(5_000)
                    })
                    .unwrap()
                })
        );
    }

    #[test]
    fn can_deposit_token() {
        let env = mock_env();
        let mut deps = mock_dependencies_lp(
            &[],
            &[(
                "creator".to_string(),
                &[("vault_token".to_string(), Uint128::new(10_000))],
            )],
            vec![(
                "creator".to_string(),
                env.clone().contract.address.into_string(),
                Uint128::new(5_000),
            )],
        );

        // inject config
        CONFIG
            .save(
                &mut deps.storage,
                &Config {
                    owner: mock_creator().sender,
                    liquidity_token: Addr::unchecked("lp_token"),
                    asset_info: AssetInfo::Token {
                        contract_addr: "vault_token".to_string(),
                    },
                    deposit_enabled: true,
                    flash_loan_enabled: true,
                    withdraw_enabled: true,
                    fee_collector_addr: Addr::unchecked("fee_collector"),
                    fees: get_fees(),
                },
            )
            .unwrap();

        // inject loan counter
        LOAN_COUNTER.save(&mut deps.storage, &0).unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_creator(),
            vault_network::vault::ExecuteMsg::Deposit {
                amount: Uint128::new(5_000),
            },
        );

        assert_eq!(
            res.unwrap(),
            Response::new()
                .add_attributes(vec![("method", "deposit"), ("amount", "5000")])
                .add_messages(vec![
                    WasmMsg::Execute {
                        contract_addr: "vault_token".to_string(),
                        funds: vec![],
                        msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                            owner: "creator".to_string(),
                            recipient: env.contract.address.into_string(),
                            amount: Uint128::new(5_000)
                        })
                        .unwrap()
                    },
                    WasmMsg::Execute {
                        contract_addr: "lp_token".to_string(),
                        funds: vec![],
                        msg: to_binary(&Cw20ExecuteMsg::Mint {
                            recipient: "creator".to_string(),
                            amount: Uint128::new(5_000)
                        })
                        .unwrap()
                    }
                ])
        )
    }

    #[test]
    fn does_verify_funds_deposited_native() {
        let (res, ..) = mock_execute(
            2,
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
            vault_network::vault::ExecuteMsg::Deposit {
                amount: Uint128::new(5_000),
            },
        );

        assert_eq!(
            res.unwrap_err(),
            VaultError::FundsMismatch {
                sent: Uint128::new(0),
                wanted: Uint128::new(5_000)
            }
        );
    }

    #[test]
    fn does_verify_funds_deposited_token() {
        let env = mock_env();
        let mut deps = mock_dependencies_lp(&[], &[], vec![]);

        // inject config
        CONFIG
            .save(
                &mut deps.storage,
                &Config {
                    owner: mock_creator().sender,
                    asset_info: AssetInfo::Token {
                        contract_addr: "vault_token".to_string(),
                    },
                    liquidity_token: Addr::unchecked("lp_token"),
                    deposit_enabled: true,
                    flash_loan_enabled: true,
                    withdraw_enabled: true,
                    fee_collector_addr: Addr::unchecked("fee_collector"),
                    fees: get_fees(),
                },
            )
            .unwrap();

        // inject loan counter
        LOAN_COUNTER.save(&mut deps.storage, &0).unwrap();

        let res = execute(
            deps.as_mut(),
            env,
            mock_creator(),
            vault_network::vault::ExecuteMsg::Deposit {
                amount: Uint128::new(5_000),
            },
        );

        assert_eq!(
            res.unwrap_err(),
            VaultError::FundsMismatch {
                sent: Uint128::new(0),
                wanted: Uint128::new(5_000)
            }
        );
    }

    #[test]
    fn cannot_deposit_when_disabled() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // inject config
        CONFIG
            .save(
                &mut deps.storage,
                &Config {
                    owner: mock_creator().sender,
                    asset_info: AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    liquidity_token: Addr::unchecked("lp_token"),
                    deposit_enabled: false,
                    flash_loan_enabled: true,
                    withdraw_enabled: true,
                    fee_collector_addr: Addr::unchecked("fee_collector_addr"),
                    fees: get_fees(),
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env,
            mock_creator(),
            vault_network::vault::ExecuteMsg::Deposit {
                amount: Uint128::new(5_000),
            },
        );

        assert_eq!(res.unwrap_err(), VaultError::DepositsDisabled {});
    }

    #[test]
    fn cannot_deposit_when_loan() {
        let mut deps = mock_dependencies();

        // inject config
        CONFIG
            .save(
                &mut deps.storage,
                &Config {
                    owner: mock_creator().sender,
                    asset_info: AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    liquidity_token: Addr::unchecked("lp_token"),
                    deposit_enabled: true,
                    flash_loan_enabled: true,
                    withdraw_enabled: true,
                    fee_collector_addr: Addr::unchecked("fee_collector_addr"),
                    fees: get_fees(),
                },
            )
            .unwrap();

        // inject loan state
        LOAN_COUNTER.save(&mut deps.storage, &2).unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_creator(),
            vault_network::vault::ExecuteMsg::Deposit {
                amount: Uint128::new(5_000),
            },
        );

        assert_eq!(res.unwrap_err(), VaultError::DepositDuringLoan {});
    }

    #[test]
    fn does_not_dilute_early_holders() {
        let env = mock_env();
        // simulate a vault with first depositor having 10,000 LP tokens
        // and vault having 15,000 of asset
        // the next depositor should not deposit at a 1:1 rate for asset:LP tokens
        // otherwise, the earlier depositor will be diluted.
        let second_depositor = Addr::unchecked("depositor2");
        let mut deps = mock_dependencies_lp(
            &[
                (env.contract.address.as_ref(), &coins(15_000, "uluna")),
                (second_depositor.as_ref(), &coins(5_000, "uluna")),
            ],
            &[(
                "creator".to_string(),
                &[("lp_token".to_string(), Uint128::new(10_000))],
            )],
            vec![],
        );

        // inject lp token address to config
        CONFIG
            .save(
                &mut deps.storage,
                &Config {
                    owner: mock_creator().sender,
                    liquidity_token: Addr::unchecked("lp_token"),
                    asset_info: AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    deposit_enabled: true,
                    flash_loan_enabled: true,
                    withdraw_enabled: true,
                    fee_collector_addr: Addr::unchecked("fee_collector"),
                    fees: get_fees(),
                },
            )
            .unwrap();

        // inject loan counter
        LOAN_COUNTER.save(&mut deps.storage, &0).unwrap();

        // set protocol fee to 500 token
        COLLECTED_PROTOCOL_FEES
            .save(
                &mut deps.storage,
                &Asset {
                    info: AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    amount: Uint128::new(500),
                },
            )
            .unwrap();

        let res = deposit(
            deps.as_mut(),
            env,
            mock_info(second_depositor.as_str(), &coins(5_000, "uluna")),
            Uint128::new(5_000),
        );

        // creator has 10,000 of the LP token in a 14,500 pool
        // depositor2 should therefore get (5000 / 14500) * 10000 = 3,448 LP tokens
        // this leaves creator with 10,000 / 13,448 of the total LP supply or 14,500 tokens
        // depositor2 is entitled to 3,448 / 13,448 of the total LP supply or 4,999 tokens

        assert_eq!(
            res.unwrap(),
            Response::new()
                .add_attributes(vec![("method", "deposit"), ("amount", "5000")])
                .add_message(WasmMsg::Execute {
                    contract_addr: "lp_token".to_string(),
                    funds: vec![],
                    msg: to_binary(&Cw20ExecuteMsg::Mint {
                        recipient: second_depositor.to_string(),
                        amount: Uint128::new(3_448)
                    })
                    .unwrap()
                })
        );
    }
}
