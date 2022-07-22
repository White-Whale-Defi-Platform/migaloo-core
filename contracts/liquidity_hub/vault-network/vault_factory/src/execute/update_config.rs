use cosmwasm_std::{DepsMut, MessageInfo, Response};

use crate::{
    err::{StdResult, VaultFactoryError},
    state::CONFIG,
};

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    new_owner: Option<String>,
) -> StdResult<Response> {
    let new_config = CONFIG.update::<_, VaultFactoryError>(deps.storage, |mut config| {
        // check that sender is the owner
        if info.sender != config.owner {
            return Err(VaultFactoryError::Unauthorized {});
        }

        if let Some(new_owner) = new_owner {
            config.owner = deps.api.addr_validate(&new_owner)?;
        };

        Ok(config)
    })?;

    Ok(Response::new().add_attributes(vec![
        ("method", "update_config"),
        ("owner", &new_config.owner.into_string()),
    ]))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{from_binary, testing::mock_info, Addr, Response};
    use vault_network::vault_factory::{ExecuteMsg, QueryMsg};

    use crate::{
        contract::{execute, query},
        err::VaultFactoryError,
        state::{Config, CONFIG},
        tests::{mock_creator, mock_execute, mock_instantiate::mock_instantiate},
    };

    #[test]
    fn does_update_owner() {
        let (res, deps, env) = mock_execute(
            1,
            2,
            ExecuteMsg::UpdateConfig {
                owner: Some("other_acc".to_string()),
            },
        );

        // check response
        assert_eq!(
            res.unwrap(),
            Response::new()
                .add_attributes(vec![("method", "update_config"), ("owner", "other_acc")])
        );

        // check query
        let config: Config =
            from_binary(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap()).unwrap();
        assert_eq!(config.owner, Addr::unchecked("other_acc"));

        // check storage
        let config = CONFIG.load(&deps.storage).unwrap();
        assert_eq!(config.owner, Addr::unchecked("other_acc"));
    }

    #[test]
    fn does_allow_empty_owner_update() {
        let (res, deps, env) = mock_execute(1, 2, ExecuteMsg::UpdateConfig { owner: None });

        // check response
        assert_eq!(
            res.unwrap(),
            Response::new().add_attributes(vec![
                ("method", "update_config"),
                ("owner", &mock_creator().sender.to_string())
            ])
        );

        // check query
        let config: Config =
            from_binary(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap()).unwrap();
        assert_eq!(config.owner, mock_creator().sender);

        // check storage
        let config = CONFIG.load(&deps.storage).unwrap();
        assert_eq!(config.owner, mock_creator().sender);
    }

    #[test]
    fn unauthorized_update_errors() {
        let (mut deps, env) = mock_instantiate(1, 2);

        let unauthorized_sender = mock_info("bad_actor", &[]);

        let res = execute(
            deps.as_mut(),
            env,
            unauthorized_sender.clone(),
            ExecuteMsg::UpdateConfig {
                owner: Some(unauthorized_sender.sender.to_string()),
            },
        )
        .unwrap_err();
        assert_eq!(res, VaultFactoryError::Unauthorized {});
    }
}
