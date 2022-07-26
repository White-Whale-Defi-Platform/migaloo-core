use cosmwasm_std::{
    coins, to_binary, Binary, CosmosMsg, DepsMut, Env, MessageInfo, Response, Uint128, WasmMsg,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};
use terraswap::asset::AssetInfo;
use vault_network::vault::{CallbackMsg, ExecuteMsg};

use crate::{
    error::{StdResult, VaultError},
    state::CONFIG,
};

pub fn flash_loan(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    msg: Binary,
) -> StdResult<Response> {
    // check that flash loans are enabled
    let config = CONFIG.load(deps.storage)?;
    if !config.flash_loan_enabled {
        return Err(VaultError::FlashLoansDisabled {});
    }

    // store current balance for after trade profit check
    let old_balance = match config.asset_info.clone() {
        AssetInfo::NativeToken { denom } => {
            deps.querier
                .query_balance(env.contract.address.clone(), denom)?
                .amount
        }
        AssetInfo::Token { contract_addr } => {
            let resp: BalanceResponse = deps.querier.query_wasm_smart(
                contract_addr,
                &Cw20QueryMsg::Balance {
                    address: env.contract.address.clone().into_string(),
                },
            )?;
            resp.balance
        }
    };

    let mut messages: Vec<CosmosMsg> = vec![];

    // create message to send funds to sender if cw20 token
    if let AssetInfo::Token { contract_addr } = config.asset_info.clone() {
        let loan_msg = WasmMsg::Execute {
            contract_addr,
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.clone().into_string(),
                amount,
            })?,
            funds: vec![],
        }
        .into();
        messages.push(loan_msg);
    };

    // get funds to send to callback (if native token then send in the callback msg)
    let callback_funds = match config.asset_info {
        AssetInfo::Token { .. } => vec![],
        AssetInfo::NativeToken { denom } => coins(amount.u128(), denom),
    };

    // add callback msg to messages
    messages.push(
        WasmMsg::Execute {
            contract_addr: info.sender.into_string(),
            msg,
            funds: callback_funds,
        }
        .into(),
    );

    // call after trade msg
    messages.push(
        WasmMsg::Execute {
            contract_addr: env.contract.address.into_string(),
            msg: to_binary(&ExecuteMsg::Callback(CallbackMsg::AfterTrade {
                old_balance,
            }))?,
            funds: vec![],
        }
        .into(),
    );

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        ("method", "flash_loan"),
        ("amount", &amount.to_string()),
    ]))
}

#[cfg(test)]
mod test {
    use cosmwasm_std::{
        coins,
        testing::{mock_dependencies, mock_dependencies_with_balance, mock_env},
        to_binary, Addr, BankMsg, Response, Uint128, WasmMsg,
    };
    use terraswap::asset::AssetInfo;

    use crate::{
        contract::{execute, instantiate},
        error::VaultError,
        state::{Config, CONFIG},
        tests::{mock_creator, mock_dependencies_lp},
    };

    #[test]
    fn cannot_loan_when_disabled() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        CONFIG
            .save(
                &mut deps.storage,
                &Config {
                    owner: mock_creator().sender,
                    liquidity_token: Addr::unchecked("lp_token"),
                    asset_info: AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    flash_loan_enabled: false,
                    deposit_enabled: true,
                    withdraw_enabled: true,
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env,
            mock_creator(),
            vault_network::vault::ExecuteMsg::FlashLoan {
                amount: Uint128::new(5_000),
                msg: to_binary(&BankMsg::Burn { amount: vec![] }).unwrap(),
            },
        );

        assert_eq!(res.unwrap_err(), VaultError::FlashLoansDisabled {})
    }

    #[test]
    fn can_loan_native() {
        let mut deps = mock_dependencies_with_balance(&coins(10_000, "uluna"));
        let env = mock_env();

        let callback_msg = to_binary(&BankMsg::Burn { amount: vec![] }).unwrap();

        instantiate(
            deps.as_mut(),
            env.clone(),
            mock_creator(),
            vault_network::vault::InstantiateMsg {
                owner: mock_creator().sender.into_string(),
                token_id: 2,
                asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
            },
        )
        .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_creator(),
            vault_network::vault::ExecuteMsg::FlashLoan {
                amount: Uint128::new(5_000),
                msg: callback_msg.clone(),
            },
        );

        // check old balance
        assert_eq!(
            res.unwrap(),
            Response::new()
                .add_attributes(vec![("method", "flash_loan"), ("amount", "5000")])
                .add_messages(vec![
                    WasmMsg::Execute {
                        contract_addr: mock_creator().sender.into_string(),
                        msg: callback_msg,
                        funds: coins(5_000, "uluna")
                    },
                    WasmMsg::Execute {
                        contract_addr: env.contract.address.into_string(),
                        funds: vec![],
                        msg: to_binary(&vault_network::vault::ExecuteMsg::Callback(
                            vault_network::vault::CallbackMsg::AfterTrade {
                                old_balance: Uint128::new(10_000)
                            }
                        ))
                        .unwrap()
                    }
                ])
        );
    }

    #[test]
    fn can_loan_token() {
        let env = mock_env();
        let mut deps = mock_dependencies_lp(
            &[],
            &[(
                env.clone().contract.address.into_string(),
                &[("vault_token".to_string(), Uint128::new(10_000))],
            )],
            vec![],
        );

        let callback_msg = to_binary(&BankMsg::Burn { amount: vec![] }).unwrap();

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
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_creator(),
            vault_network::vault::ExecuteMsg::FlashLoan {
                amount: Uint128::new(5_000),
                msg: callback_msg.clone(),
            },
        );

        // check old balance
        assert_eq!(
            res.unwrap(),
            Response::new()
                .add_attributes(vec![("method", "flash_loan"), ("amount", "5000")])
                .add_messages(vec![
                    WasmMsg::Execute {
                        contract_addr: "vault_token".to_string(),
                        funds: vec![],
                        msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                            recipient: mock_creator().sender.into_string(),
                            amount: Uint128::new(5_000)
                        })
                        .unwrap()
                    },
                    WasmMsg::Execute {
                        contract_addr: mock_creator().sender.into_string(),
                        msg: callback_msg,
                        funds: vec![]
                    },
                    WasmMsg::Execute {
                        contract_addr: env.contract.address.into_string(),
                        funds: vec![],
                        msg: to_binary(&vault_network::vault::ExecuteMsg::Callback(
                            vault_network::vault::CallbackMsg::AfterTrade {
                                old_balance: Uint128::new(10_000)
                            }
                        ))
                        .unwrap()
                    }
                ])
        );
    }
}
