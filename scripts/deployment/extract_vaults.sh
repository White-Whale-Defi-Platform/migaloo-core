#!/usr/bin/env bash
set -e

# Import the deploy_liquidity_hub script
deployment_script_dir=$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
project_root_path=$(realpath "$0" | sed 's|\(.*\)/.*|\1|' | cd ../ | pwd)

# Displays tool usage
function display_usage() {
  echo "WW Vault Extractor"
  echo -e "\nUsage:./extract_vaults.sh [flags].\n"
  echo -e "Available flags:\n"
  echo -e "  -h \thelp"
  echo -e "  -c \tThe chain where you want to extract vaults on (juno|juno-testnet|terra|terra-testnet|... check chain_env.sh for the complete list of supported chains)"
}

function extract_vaults() {
  mkdir -p $project_root_path/scripts/deployment/output
  output_file=$project_root_path/scripts/deployment/output/"$CHAIN_ID"_vaults.json
  deployment_file=$project_root_path/scripts/deployment/output/"$CHAIN_ID"_liquidity_hub_contracts.json
  if [[ ! -f "$output_file" ]]; then
    # create file to dump results into
    echo '{"vaults": []}' | jq '.' >$output_file
  fi

  vault_factory_addr=$(jq -r '.contracts[] | select (.wasm == "vault_factory.wasm") | .contract_address' $deployment_file)
  local limit=30
  query='{"vaults":{"limit": '$limit'}}'
  local vaults=$($BINARY query wasm contract-state smart $vault_factory_addr "$query" --node $RPC --output json | jq '.data.vaults')
  vault_code_id=$(jq -r '.contracts[] | select (.wasm == "vault.wasm") | .code_id' $deployment_file)
  lp_code_id=$(jq -r '.contracts[] | select (.wasm == "terraswap_token.wasm") | .code_id' $deployment_file)

  echo -e "\nExtracting vaults info...\n"

  jq '[.[] | {vault: .vault, asset_info: .asset_info}]' <<<"$vaults" >$output_file
  jq --arg vault_factory_addr "$vault_factory_addr" \
    --arg chain "$CHAIN_ID" \
    '{chain: $chain, vault_factory_addr: $vault_factory_addr, vaults: [.[] | {vault: .vault, asset_info: .asset_info}]}' <<<$vaults >$output_file

  echo -e "\n**** Extracted vault data on $CHAIN_ID successfully ****\n"
  jq '.' $output_file
}

if [ -z $1 ]; then
  display_usage
  exit 0
fi

# get args
optstring=':c:h'
while getopts $optstring arg; do
  case "$arg" in
  c)
    chain=$OPTARG
    source $deployment_script_dir/deploy_env/chain_env.sh
    init_chain_env $OPTARG
    extract_vaults
    ;;
  h)
    display_usage
    exit 0
    ;;
  :)
    echo "Must supply an argument to -$OPTARG" >&2
    exit 1
    ;;
  ?)
    echo "Invalid option: -${OPTARG}"
    display_usage
    exit 2
    ;;
  esac
done
