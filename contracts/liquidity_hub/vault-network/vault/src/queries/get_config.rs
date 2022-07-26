use cosmwasm_std::{to_binary, Binary, Deps};

use crate::{error::StdResult, state::CONFIG};

pub fn get_config(deps: Deps) -> StdResult<Binary> {
    Ok(to_binary(&CONFIG.load(deps.storage)?)?)
}

#[cfg(test)]
mod test {
    use cosmwasm_std::{
        from_binary,
        testing::{mock_dependencies, mock_env},
        Addr,
    };
    use terraswap::asset::AssetInfo;

    use crate::{
        contract::query,
        state::{Config, CONFIG},
        tests::mock_creator,
    };

    #[test]
    fn does_get_config() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let config = Config {
            owner: mock_creator().sender,
            liquidity_token: Addr::unchecked("lp_token"),
            asset_info: AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
            deposit_enabled: false,
            flash_loan_enabled: true,
            withdraw_enabled: false,
        };

        CONFIG.save(&mut deps.storage, &config).unwrap();

        let res: Config = from_binary(
            &query(
                deps.as_ref(),
                env,
                vault_network::vault::QueryMsg::Config {},
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(res, config);
    }
}
