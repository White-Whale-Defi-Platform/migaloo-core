use cosmwasm_std::{Addr, Coin, StdResult, Timestamp, Uint128};
use cw20::{BalanceResponse, Cw20Coin, MinterResponse};
use cw_multi_test::{App, AppBuilder, AppResponse, BankKeeper, Executor};

use white_whale::pool_network::asset::{Asset, AssetInfo};
use white_whale::pool_network::incentive::{Curve, Flow, FlowResponse};
use white_whale::pool_network::incentive_factory::{
    IncentiveResponse, IncentivesResponse, InstantiateMsg,
};

use crate::error::ContractError;
use crate::tests::suite_contracts::{
    cw20_token_contract, fee_collector_contract, incentive_contract, incentive_factory_contract,
};

pub struct TestingSuite {
    app: App,
    pub senders: [Addr; 3],
    pub incentive_factory_addr: Addr,
    pub cw20_tokens: Vec<Addr>,
}

/// helpers
impl TestingSuite {
    pub(crate) fn creator(&mut self) -> Addr {
        self.senders.first().unwrap().clone()
    }

    pub(crate) fn fast_forward(&mut self, seconds: u64) -> &mut Self {
        let mut block_info = self.app.block_info();
        block_info.time = block_info.time.plus_nanos(seconds * 1_000_000_000);
        self.app.set_block(block_info);

        self
    }

    pub(crate) fn set_time(&mut self, timestamp: Timestamp) -> &mut Self {
        let mut block_info = self.app.block_info();
        block_info.time = timestamp;
        self.app.set_block(block_info);

        self
    }

    pub(crate) fn get_time(&mut self) -> Timestamp {
        self.app.block_info().time
    }
}

/// instantiate / execute messages
impl TestingSuite {
    pub(crate) fn default() -> Self {
        let sender_1 = Addr::unchecked("alice");
        let sender_2 = Addr::unchecked("bob");
        let sender_3 = Addr::unchecked("carol");

        Self {
            app: App::default(),
            senders: [sender_1, sender_2, sender_3],
            incentive_factory_addr: Addr::unchecked(""),
            cw20_tokens: vec![],
        }
    }

    pub(crate) fn default_with_balances(initial_balance: Vec<Coin>) -> Self {
        let sender_1 = Addr::unchecked("alice");
        let sender_2 = Addr::unchecked("bob");
        let sender_3 = Addr::unchecked("carol");

        let bank = BankKeeper::new();

        let balances = vec![
            (sender_1.clone(), initial_balance.clone()),
            (sender_2.clone(), initial_balance.clone()),
            (sender_3.clone(), initial_balance.clone()),
        ];

        let app = AppBuilder::new()
            .with_bank(bank)
            .build(|router, _api, storage| {
                balances.into_iter().for_each(|(account, amount)| {
                    router.bank.init_balance(storage, &account, amount).unwrap()
                });
            });

        Self {
            app,
            senders: [sender_1, sender_2, sender_3],
            incentive_factory_addr: Addr::unchecked(""),
            cw20_tokens: vec![],
        }
    }

    #[track_caller]
    pub(crate) fn instantiate_default(&mut self) -> &mut Self {
        let incentive_id = self.app.store_code(incentive_contract());
        let fee_collector_addr =
            instantiate_contract(self, InstatiateContract::FeeCollector {}).unwrap();

        let cw20_token = instantiate_contract(
            self,
            InstatiateContract::CW20 {
                name: "uLP".to_string(),
                symbol: "uLP".to_string(),
                decimals: 6,
                initial_balances: vec![
                    Cw20Coin {
                        address: self.senders[0].to_string(),
                        amount: Uint128::new(1_000_000_000_000u128),
                    },
                    Cw20Coin {
                        address: self.senders[1].to_string(),
                        amount: Uint128::new(1_000_000_000_000u128),
                    },
                    Cw20Coin {
                        address: self.senders[2].to_string(),
                        amount: Uint128::new(1_000_000_000_000u128),
                    },
                ],
                mint: Some(MinterResponse {
                    minter: self.senders[0].to_string(),
                    cap: None,
                }),
            },
        )
        .unwrap();

        self.cw20_tokens = vec![cw20_token.clone()];

        // 17 May 2023 17:00:00 UTC
        let timestamp = Timestamp::from_seconds(1684342800u64);
        self.set_time(timestamp);

        self.instantiate(
            fee_collector_addr.to_string(),
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uwhale".to_string(),
                },
                amount: Uint128::new(1_000u128),
            },
            7u64,
            incentive_id,
            100u64,
            86400,
            259200,
        )
    }

    #[track_caller]
    pub(crate) fn instantiate(
        &mut self,
        fee_collector_addr: String,
        create_flow_fee: Asset,
        max_concurrent_flows: u64,
        incentive_code_id: u64,
        max_flow_start_time_buffer: u64,
        min_unbonding_duration: u64,
        max_unbonding_duration: u64,
    ) -> &mut Self {
        let incentive_factory_addr = instantiate_contract(
            self,
            InstatiateContract::IncentiveFactory {
                fee_collector_addr,
                create_flow_fee,
                max_concurrent_flows,
                incentive_code_id,
                max_flow_start_time_buffer,
                min_unbonding_duration,
                max_unbonding_duration,
            },
        )
        .unwrap();

        self.incentive_factory_addr = incentive_factory_addr;
        self
    }

    #[track_caller]
    pub(crate) fn instantiate_err(
        &mut self,
        fee_collector_addr: String,
        create_flow_fee: Asset,
        max_concurrent_flows: u64,
        incentive_code_id: u64,
        max_flow_start_time_buffer: u64,
        min_unbonding_duration: u64,
        max_unbonding_duration: u64,
        error: impl Fn(anyhow::Error),
    ) -> &mut Self {
        let err = instantiate_contract(
            self,
            InstatiateContract::IncentiveFactory {
                fee_collector_addr,
                create_flow_fee,
                max_concurrent_flows,
                incentive_code_id,
                max_flow_start_time_buffer,
                min_unbonding_duration,
                max_unbonding_duration,
            },
        )
        .unwrap_err();

        error(err);

        self
    }

    #[track_caller]
    pub(crate) fn create_lp_tokens(&mut self) -> &mut Self {
        let mut lp_tokens = self.cw20_tokens.clone();

        for _ in 0..9 {
            let cw20_token = instantiate_contract(
                self,
                InstatiateContract::CW20 {
                    name: "uLP".to_string(),
                    symbol: "uLP".to_string(),
                    decimals: 6,
                    initial_balances: vec![
                        Cw20Coin {
                            address: self.senders[0].to_string(),
                            amount: Uint128::new(1_000_000_000_000u128),
                        },
                        Cw20Coin {
                            address: self.senders[1].to_string(),
                            amount: Uint128::new(1_000_000_000_000u128),
                        },
                        Cw20Coin {
                            address: self.senders[2].to_string(),
                            amount: Uint128::new(1_000_000_000_000u128),
                        },
                    ],
                    mint: Some(MinterResponse {
                        minter: self.senders[0].to_string(),
                        cap: None,
                    }),
                },
            )
            .unwrap();

            lp_tokens.push(cw20_token.clone());
        }

        self.cw20_tokens = lp_tokens;

        self
    }
}

/// execute messages
impl TestingSuite {
    pub(crate) fn create_incentive(
        &mut self,
        sender: Addr,
        lp_address: AssetInfo,
        result: impl Fn(Result<AppResponse, anyhow::Error>),
    ) -> &mut Self {
        let msg = white_whale::pool_network::incentive_factory::ExecuteMsg::CreateIncentive {
            lp_address,
        };

        result(self.app.execute_contract(
            sender,
            self.incentive_factory_addr.clone(),
            &msg,
            &vec![],
        ));

        self
    }

    pub(crate) fn open_incentive_flow(
        &mut self,
        sender: Addr,
        incentive_addr: Addr,
        start_timestamp: Option<u64>,
        end_timestamp: u64,
        curve: Curve,
        flow_asset: Asset,
        funds: &Vec<Coin>,
        result: impl Fn(Result<AppResponse, anyhow::Error>),
    ) -> &mut Self {
        let msg = white_whale::pool_network::incentive::ExecuteMsg::OpenFlow {
            start_timestamp,
            end_timestamp,
            curve,
            flow_asset,
        };

        result(
            self.app
                .execute_contract(sender, incentive_addr, &msg, funds),
        );

        self
    }

    pub(crate) fn close_incentive_flow(
        &mut self,
        sender: Addr,
        incentive_addr: Addr,
        flow_id: u64,
        result: impl Fn(Result<AppResponse, anyhow::Error>),
    ) -> &mut Self {
        let msg = white_whale::pool_network::incentive::ExecuteMsg::CloseFlow { flow_id };

        result(
            self.app
                .execute_contract(sender, incentive_addr, &msg, &vec![]),
        );

        self
    }
}

/// queries
impl TestingSuite {
    pub(crate) fn query_incentive(
        &mut self,
        lp_address: AssetInfo,
        result: impl Fn(StdResult<IncentiveResponse>),
    ) -> &mut Self {
        let incentive_response: StdResult<IncentiveResponse> = self.app.wrap().query_wasm_smart(
            &self.incentive_factory_addr,
            &white_whale::pool_network::incentive_factory::QueryMsg::Incentive { lp_address },
        );

        result(incentive_response);

        self
    }

    pub(crate) fn query_incentives(
        &mut self,
        start_after: Option<AssetInfo>,
        limit: Option<u32>,
        result: impl Fn(StdResult<IncentivesResponse>),
    ) -> &mut Self {
        let incentive_response: StdResult<IncentivesResponse> = self.app.wrap().query_wasm_smart(
            &self.incentive_factory_addr,
            &white_whale::pool_network::incentive_factory::QueryMsg::Incentives {
                start_after,
                limit,
            },
        );

        result(incentive_response);

        self
    }

    pub(crate) fn query_flow(
        &mut self,
        incentive_addr: Addr,
        flow_id: u64,
        result: impl Fn(StdResult<Option<FlowResponse>>),
    ) -> &mut Self {
        let flow_response: StdResult<Option<FlowResponse>> = self.app.wrap().query_wasm_smart(
            incentive_addr,
            &white_whale::pool_network::incentive::QueryMsg::Flow { flow_id },
        );

        result(flow_response);

        self
    }

    pub(crate) fn query_flows(
        &mut self,
        incentive_addr: Addr,
        result: impl Fn(StdResult<Vec<Flow>>),
    ) -> &mut Self {
        let flows_response: StdResult<Vec<Flow>> = self.app.wrap().query_wasm_smart(
            incentive_addr,
            &white_whale::pool_network::incentive::QueryMsg::Flows {},
        );

        result(flows_response);

        self
    }

    pub(crate) fn query_incentive_factory_config(
        &mut self,
        result: impl Fn(StdResult<white_whale::pool_network::incentive_factory::ConfigResponse>),
    ) -> &mut Self {
        let config_response: StdResult<
            white_whale::pool_network::incentive_factory::ConfigResponse,
        > = self.app.wrap().query_wasm_smart(
            self.incentive_factory_addr.clone(),
            &white_whale::pool_network::incentive_factory::QueryMsg::Config {},
        );

        result(config_response);
        self
    }

    pub(crate) fn query_funds(
        &mut self,
        contract: Addr,
        asset: AssetInfo,
        result: impl Fn(Uint128),
    ) -> &mut Self {
        let funds = match asset {
            AssetInfo::Token { contract_addr } => {
                let balance_response: StdResult<BalanceResponse> =
                    self.app.wrap().query_wasm_smart(
                        contract_addr,
                        &cw20_base::msg::QueryMsg::Balance {
                            address: contract.to_string(),
                        },
                    );

                balance_response.unwrap().balance
            }
            AssetInfo::NativeToken { denom } => {
                let coin: StdResult<Coin> = self.app.wrap().query_balance(contract, denom);

                coin.unwrap().amount
            }
        };

        result(funds);
        self
    }
}

enum InstatiateContract {
    IncentiveFactory {
        fee_collector_addr: String,
        create_flow_fee: Asset,
        max_concurrent_flows: u64,
        incentive_code_id: u64,
        max_flow_start_time_buffer: u64,
        min_unbonding_duration: u64,
        max_unbonding_duration: u64,
    },
    FeeCollector,
    CW20 {
        name: String,
        symbol: String,
        decimals: u8,
        initial_balances: Vec<Cw20Coin>,
        mint: Option<MinterResponse>,
    },
}

fn instantiate_contract(
    suite: &mut TestingSuite,
    instantiate_contract: InstatiateContract,
) -> anyhow::Result<Addr> {
    match instantiate_contract {
        InstatiateContract::IncentiveFactory {
            fee_collector_addr,
            create_flow_fee,
            max_concurrent_flows,
            incentive_code_id,
            max_flow_start_time_buffer,
            min_unbonding_duration,
            max_unbonding_duration,
        } => {
            let msg = InstantiateMsg {
                fee_collector_addr,
                create_flow_fee,
                max_concurrent_flows,
                incentive_code_id,
                max_flow_start_time_buffer,
                min_unbonding_duration,
                max_unbonding_duration,
            };

            let incentive_factory_id = suite.app.store_code(incentive_factory_contract());

            suite.app.instantiate_contract(
                incentive_factory_id,
                suite.senders[0].clone(),
                &msg,
                &[],
                "mock incentive factory",
                Some(suite.senders[0].clone().into_string()),
            )
        }
        InstatiateContract::FeeCollector => {
            let msg = white_whale::fee_collector::InstantiateMsg {};

            let fee_collector_id = suite.app.store_code(fee_collector_contract());

            suite.app.instantiate_contract(
                fee_collector_id,
                suite.senders[0].clone(),
                &msg,
                &[],
                "mock fee collector",
                Some(suite.senders[0].clone().into_string()),
            )
        }
        InstatiateContract::CW20 {
            name,
            symbol,
            decimals,
            initial_balances,
            mint,
        } => {
            let msg = white_whale::pool_network::token::InstantiateMsg {
                name,
                symbol,
                decimals,
                initial_balances,
                mint,
            };

            let cw20_token_id = suite.app.store_code(cw20_token_contract());

            suite.app.instantiate_contract(
                cw20_token_id,
                suite.senders[0].clone(),
                &msg,
                &[],
                "mock cw20 token",
                Some(suite.senders[0].clone().into_string()),
            )
        }
    }
}
