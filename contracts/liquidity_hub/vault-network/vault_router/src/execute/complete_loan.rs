use classic_bindings::TerraQuery;
use cosmwasm_std::{
    attr, coins, to_binary, Addr, BankMsg, CosmosMsg, DepsMut, Env, MessageInfo, Response, WasmMsg,
};
use white_whale::pool_network::asset::{Asset, AssetInfo};
use white_whale::vault_network::vault::PaybackAmountResponse;

use crate::err::{StdResult, VaultRouterError};

pub fn complete_loan(
    deps: DepsMut<TerraQuery>,
    env: Env,
    info: MessageInfo,
    initiator: Addr,
    assets: Vec<(String, Asset)>,
) -> StdResult<Response> {
    // check that the contract itself is executing this message
    if info.sender != env.contract.address {
        return Err(VaultRouterError::Unauthorized {});
    }

    let mut attributes = vec![];

    // pay back loans and profit
    let messages: Vec<Vec<CosmosMsg>> = assets
        .into_iter()
        .map(|(vault, loaned_asset)| {
            let payback_amount: PaybackAmountResponse = deps.querier.query_wasm_smart(
                vault.clone(),
                &white_whale::vault_network::vault::QueryMsg::GetPaybackAmount {
                    amount: loaned_asset.amount,
                },
            )?;

            // calculate amount router has after performing flash-loan
            let final_amount = match &loaned_asset.info {
                AssetInfo::NativeToken { denom } => {
                    let amount = deps
                        .querier
                        .query_balance(env.contract.address.clone(), denom)?;

                    amount.amount
                }
                AssetInfo::Token { contract_addr } => {
                    let res: cw20::BalanceResponse = deps.querier.query_wasm_smart(
                        contract_addr,
                        &cw20::Cw20QueryMsg::Balance {
                            address: env.contract.address.clone().into_string(),
                        },
                    )?;

                    res.balance
                }
            };

            let profit_amount = final_amount
                .checked_sub(payback_amount.payback_amount)
                .map_err(|_| VaultRouterError::NegativeProfit {
                    input: loaned_asset.clone(),
                    output_amount: final_amount,
                    required_amount: payback_amount.payback_amount,
                })?;

            attributes.push(attr(
                "payback_amount",
                payback_amount.payback_amount.to_string(),
            ));
            attributes.push(attr("profit_amount", profit_amount.to_string()));

            let mut response_messages: Vec<CosmosMsg> = vec![];
            let payback_loan_asset = Asset {
                info: loaned_asset.info.clone(),
                amount: payback_amount.payback_amount,
            };

            let payback_loan_msg =
                payback_loan_asset.into_msg(&deps.querier, deps.api.addr_validate(&vault)?);

            response_messages.push(payback_loan_msg?);

            // add profit message if non-zero profit
            if !profit_amount.is_zero() {
                let profit_asset = Asset {
                    info: loaned_asset.info.clone(),
                    amount: profit_amount.clone(),
                };

                let profit_payback_msg = profit_asset.into_msg(&deps.querier, initiator.clone());

                response_messages.push(profit_payback_msg?);
            }

            Ok(response_messages)
        })
        .collect::<StdResult<Vec<Vec<_>>>>()?;

    Ok(Response::new()
        .add_messages(messages.concat())
        .add_attributes(vec![("method", "complete_loan")])
        .add_attributes(attributes))
}

// #[cfg(test)]
// #[cfg(not(target_arch = "wasm32"))]
// mod tests {
//     use cosmwasm_std::{coin, coins, Uint128};
//     use cw_multi_test::Executor;
//     use white_whale::pool_network::asset::{Asset, AssetInfo};
//     use white_whale::vault_network::vault_router::ExecuteMsg;
//
//     use crate::{
//         err::VaultRouterError,
//         tests::{
//             mock_admin, mock_app_with_balance, mock_creator,
//             mock_instantiate::{app_mock_instantiate, AppInstantiateResponse},
//         },
//     };
//
//     #[test]
//     fn does_handle_native_zero_profit_loan() {
//         let mut app = mock_app_with_balance(vec![(mock_admin(), coins(10_532, "uluna"))]);
//
//         let AppInstantiateResponse {
//             router_addr,
//             native_vault_addr,
//             ..
//         } = app_mock_instantiate(&mut app);
//
//         // should succeed at paying back loan
//         // first give the router the 500 + 32 payback amount
//         app.send_tokens(mock_admin(), router_addr.clone(), &coins(532, "uluna"))
//             .unwrap();
//
//         app.execute_contract(
//             router_addr.clone(),
//             router_addr,
//             &ExecuteMsg::CompleteLoan {
//                 initiator: mock_creator().sender,
//                 loaned_assets: vec![(
//                     native_vault_addr.clone().into_string(),
//                     Asset {
//                         amount: Uint128::new(500),
//                         info: AssetInfo::NativeToken {
//                             denom: "uluna".to_string(),
//                         },
//                     },
//                 )],
//             },
//             &[],
//         )
//         .unwrap();
//
//         // native vault should have the 10k deposit + 532 returned from loan
//         assert_eq!(
//             app.wrap()
//                 .query_balance(native_vault_addr, "uluna")
//                 .unwrap(),
//             coin(10_532, "uluna")
//         );
//     }
//
//     #[test]
//     fn does_handle_native_profit_loan() {
//         let mut app = mock_app_with_balance(vec![(mock_admin(), coins(11_000, "uluna"))]);
//
//         let AppInstantiateResponse {
//             router_addr,
//             native_vault_addr,
//             ..
//         } = app_mock_instantiate(&mut app);
//
//         // should succeed at paying back loan
//         // payback amount is 532, but there is an excess profit of 468
//         app.send_tokens(mock_admin(), router_addr.clone(), &coins(1_000, "uluna"))
//             .unwrap();
//
//         app.execute_contract(
//             router_addr.clone(),
//             router_addr,
//             &ExecuteMsg::CompleteLoan {
//                 initiator: mock_creator().sender,
//                 loaned_assets: vec![(
//                     native_vault_addr.clone().into_string(),
//                     Asset {
//                         amount: Uint128::new(500),
//                         info: AssetInfo::NativeToken {
//                             denom: "uluna".to_string(),
//                         },
//                     },
//                 )],
//             },
//             &[],
//         )
//         .unwrap();
//
//         // native vault should have the 10k deposit + 532 returned from loan
//         assert_eq!(
//             app.wrap()
//                 .query_balance(native_vault_addr, "uluna")
//                 .unwrap(),
//             coin(10_532, "uluna")
//         );
//
//         // profit should have been returned back to loan creator
//         assert_eq!(
//             app.wrap()
//                 .query_balance(mock_creator().sender, "uluna")
//                 .unwrap(),
//             coin(468, "uluna")
//         );
//     }
//
//     #[test]
//     fn does_handle_token_zero_profit_loan() {
//         let mut app = mock_app_with_balance(vec![(mock_admin(), coins(10_000, "uluna"))]);
//
//         let AppInstantiateResponse {
//             router_addr,
//             token_addr,
//             token_vault_addr,
//             ..
//         } = app_mock_instantiate(&mut app);
//
//         // should succeed at paying back loan
//         // first give the router the 500 + 32 payback amount
//         app.execute_contract(
//             mock_admin(),
//             token_addr.clone(),
//             &cw20::Cw20ExecuteMsg::Transfer {
//                 recipient: router_addr.clone().into_string(),
//                 amount: Uint128::new(532),
//             },
//             &[],
//         )
//         .unwrap();
//
//         app.execute_contract(
//             router_addr.clone(),
//             router_addr,
//             &ExecuteMsg::CompleteLoan {
//                 initiator: mock_creator().sender,
//                 loaned_assets: vec![(
//                     token_vault_addr.clone().into_string(),
//                     Asset {
//                         amount: Uint128::new(500),
//                         info: AssetInfo::Token {
//                             contract_addr: token_addr.clone().into_string(),
//                         },
//                     },
//                 )],
//             },
//             &[],
//         )
//         .unwrap();
//
//         // token vault should have the 10k deposit + 532 returned from loan
//         let vault_balance: cw20::BalanceResponse = app
//             .wrap()
//             .query_wasm_smart(
//                 token_addr,
//                 &cw20::Cw20QueryMsg::Balance {
//                     address: token_vault_addr.into_string(),
//                 },
//             )
//             .unwrap();
//         assert_eq!(vault_balance.balance.u128(), 10_532);
//     }
//
//     #[test]
//     fn does_handle_token_profit_loan() {
//         let mut app = mock_app_with_balance(vec![(mock_admin(), coins(10_000, "uluna"))]);
//
//         let AppInstantiateResponse {
//             router_addr,
//             token_addr,
//             token_vault_addr,
//             ..
//         } = app_mock_instantiate(&mut app);
//
//         // should succeed at paying back loan
//         // payback amount is 532, but there is an excess profit of 468
//         app.execute_contract(
//             mock_admin(),
//             token_addr.clone(),
//             &cw20::Cw20ExecuteMsg::Transfer {
//                 recipient: router_addr.clone().into_string(),
//                 amount: Uint128::new(1_000),
//             },
//             &[],
//         )
//         .unwrap();
//
//         app.execute_contract(
//             router_addr.clone(),
//             router_addr,
//             &ExecuteMsg::CompleteLoan {
//                 initiator: mock_creator().sender,
//                 loaned_assets: vec![(
//                     token_vault_addr.clone().into_string(),
//                     Asset {
//                         amount: Uint128::new(500),
//                         info: AssetInfo::Token {
//                             contract_addr: token_addr.clone().into_string(),
//                         },
//                     },
//                 )],
//             },
//             &[],
//         )
//         .unwrap();
//
//         // token vault should have the 10k deposit + 532 returned from loan
//         let vault_balance: cw20::BalanceResponse = app
//             .wrap()
//             .query_wasm_smart(
//                 token_addr.clone(),
//                 &cw20::Cw20QueryMsg::Balance {
//                     address: token_vault_addr.into_string(),
//                 },
//             )
//             .unwrap();
//         assert_eq!(vault_balance.balance.u128(), 10_532);
//
//         // profit should have been returned back to loan creator
//         let user_balance: cw20::BalanceResponse = app
//             .wrap()
//             .query_wasm_smart(
//                 token_addr,
//                 &cw20::Cw20QueryMsg::Balance {
//                     address: mock_creator().sender.into_string(),
//                 },
//             )
//             .unwrap();
//         assert_eq!(user_balance.balance.u128(), 468);
//     }
//
//     #[test]
//     fn does_error_on_negative_profit() {
//         let mut app = mock_app_with_balance(vec![(mock_admin(), coins(10_005, "uluna"))]);
//
//         let AppInstantiateResponse {
//             router_addr,
//             native_vault_addr,
//             ..
//         } = app_mock_instantiate(&mut app);
//
//         // give the router 5 uluna
//         app.send_tokens(mock_admin(), router_addr.clone(), &coins(5, "uluna"))
//             .unwrap();
//
//         // now try to complete loan
//         let err = app
//             .execute_contract(
//                 router_addr.clone(),
//                 router_addr,
//                 &ExecuteMsg::CompleteLoan {
//                     initiator: mock_creator().sender,
//                     loaned_assets: vec![(
//                         native_vault_addr.into_string(),
//                         Asset {
//                             amount: Uint128::new(1_000),
//                             info: AssetInfo::NativeToken {
//                                 denom: "uluna".to_string(),
//                             },
//                         },
//                     )],
//                 },
//                 &[],
//             )
//             .unwrap_err();
//
//         assert_eq!(
//             err.downcast::<VaultRouterError>().unwrap(),
//             VaultRouterError::NegativeProfit {
//                 input: Asset {
//                     amount: Uint128::new(1_000),
//                     info: AssetInfo::NativeToken {
//                         denom: "uluna".to_string()
//                     }
//                 },
//                 output_amount: Uint128::new(5),
//                 required_amount: Uint128::new(1_066)
//             }
//         );
//     }
//
//     #[test]
//     fn does_require_authorization() {
//         let mut app = mock_app_with_balance(vec![(mock_admin(), coins(10_000, "uluna"))]);
//
//         let AppInstantiateResponse {
//             router_addr,
//             native_vault_addr,
//             ..
//         } = app_mock_instantiate(&mut app);
//
//         // now try to complete loan from unauthorized addr
//         let err = app
//             .execute_contract(
//                 mock_creator().sender,
//                 router_addr,
//                 &ExecuteMsg::CompleteLoan {
//                     initiator: mock_creator().sender,
//                     loaned_assets: vec![(
//                         native_vault_addr.into_string(),
//                         Asset {
//                             amount: Uint128::new(1_000),
//                             info: AssetInfo::NativeToken {
//                                 denom: "uluna".to_string(),
//                             },
//                         },
//                     )],
//                 },
//                 &[],
//             )
//             .unwrap_err();
//
//         assert_eq!(
//             err.downcast::<VaultRouterError>().unwrap(),
//             VaultRouterError::Unauthorized {}
//         );
//     }
// }
