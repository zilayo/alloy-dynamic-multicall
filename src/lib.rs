use std::fmt::Debug;

use alloy::dyn_abi::{DynSolValue, FunctionExt, JsonAbiExt};
use alloy::eips::BlockId;
use alloy::json_abi::Function;
use alloy::network::{Network, TransactionBuilder};
use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::{
    bindings::IMulticall3::{aggregate3Call, Call3},
    Failure, MulticallError, Provider, Result, MULTICALL3_ADDRESS,
};
use alloy::rpc::types::{state::StateOverride, TransactionInputKind};
use alloy::sol_types::SolCall;

/// Basic version of [alloy::providers::MulticallBuilder] to allow using multicall within type constraints.
#[derive(Debug)]
pub struct DynamicMulticallBuilder<P: Provider<N>, N: Network> {
    calls: Vec<DynCallItem>,
    provider: P,
    block: Option<BlockId>,
    state_override: Option<StateOverride>,
    address: Address,
    input_kind: TransactionInputKind,
    _pd: std::marker::PhantomData<N>,
}

impl<P, N> DynamicMulticallBuilder<P, N>
where
    P: Provider<N>,
    N: Network,
{
    /// Instantiate a new [`DynamicMulticallBuilder`]
    pub fn new(provider: P) -> Self {
        Self {
            calls: Vec::new(),
            provider,
            block: None,
            state_override: None,
            address: MULTICALL3_ADDRESS,
            input_kind: TransactionInputKind::default(),
            _pd: Default::default(),
        }
    }

    /// Adds a [`DynCallItem`] to the builder
    pub fn add_call(mut self, call: DynCallItem) -> Self {
        self.calls.push(call);

        Self {
            calls: self.calls,
            provider: self.provider,
            block: self.block,
            state_override: self.state_override,
            address: self.address,
            input_kind: self.input_kind,
            _pd: Default::default(),
        }
    }

    /// Call the `aggregate3` function
    pub async fn aggregate3(&self) -> Result<Vec<Result<Vec<DynSolValue>, Failure>>> {
        let calls = self
            .calls
            .iter()
            .map(|c| {
                let encoded_call = c.decoder.abi_encode_input(&c.params).map_err(|err| {
                    MulticallError::DecodeError(alloy::sol_types::Error::custom(err.to_string()))
                })?;

                Ok(Call3 {
                    target: c.target,
                    callData: encoded_call.into(),
                    allowFailure: c.allow_failure,
                })
            })
            .collect::<Result<Vec<Call3>, MulticallError>>()?;

        let call = aggregate3Call {
            calls: calls.to_vec(),
        };
        let results = self.build_and_call(call, None).await?;

        if results.len() != calls.len() {
            return Err(MulticallError::NoReturnData);
        }

        let mut decoded_results: Vec<Result<Vec<DynSolValue>, Failure>> =
            Vec::with_capacity(calls.len());

        for (idx, result) in results.iter().enumerate() {
            let decoded_call_result = match result.success {
                true => {
                    let decoded = self.calls[idx]
                        .decoder
                        .abi_decode_output(&result.returnData)
                        .map_err(|err| {
                            MulticallError::DecodeError(alloy::sol_types::Error::custom(
                                err.to_string(),
                            ))
                        })?;
                    Ok(decoded)
                }
                false => Err(Failure {
                    idx,
                    return_data: result.returnData.clone(),
                }),
            };

            decoded_results.push(decoded_call_result);
        }

        Ok(decoded_results)
    }

    /// Helper fn to build a tx and call the multicall contract
    async fn build_and_call<M: SolCall>(
        &self,
        call_type: M,
        value: Option<U256>,
    ) -> Result<M::Return> {
        let call = call_type.abi_encode();

        let mut tx = N::TransactionRequest::default()
            .with_to(self.address)
            .with_input_kind(Bytes::from_iter(call), self.input_kind);

        if let Some(value) = value {
            tx.set_value(value);
        }

        let mut eth_call = self.provider.root().call(tx);

        if let Some(block) = self.block {
            eth_call = eth_call.block(block);
        }

        if let Some(overrides) = self.state_override.clone() {
            eth_call = eth_call.overrides(overrides);
        }

        let res = eth_call.await.map_err(MulticallError::TransportError)?;

        M::abi_decode_returns(&res).map_err(MulticallError::DecodeError)
    }

    /// Returns a builder with empty calls.
    ///
    /// Retains previously set provider, address, block and state_override settings.
    pub fn clear(self) -> Self {
        Self {
            calls: Vec::new(),
            provider: self.provider,
            block: self.block,
            state_override: self.state_override,
            address: self.address,
            input_kind: self.input_kind,
            _pd: Default::default(),
        }
    }

    /// Get the number of calls in the builder
    pub fn len(&self) -> usize {
        self.calls.len()
    }

    /// Check if the builder is empty
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    /// Set the input kind for this builder
    pub const fn with_input_kind(mut self, input_kind: TransactionInputKind) -> Self {
        self.input_kind = input_kind;
        self
    }

    /// Get the input kind for this builder
    pub const fn input_kind(&self) -> TransactionInputKind {
        self.input_kind
    }
}

/// An individual multicall call item
#[derive(Clone)]
pub struct DynCallItem {
    target: Address,
    params: Vec<DynSolValue>,
    allow_failure: bool,
    value: U256,
    decoder: Function,
}

impl Debug for DynCallItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallItem")
            .field("target", &self.target)
            .field("allow_failure", &self.allow_failure)
            .field("value", &self.value)
            .field("params", &self.params)
            .finish()
    }
}

impl DynCallItem {
    /// Create a new [`DynCallItem`] instance.
    pub const fn new(
        target: Address,
        params: Vec<DynSolValue>,
        function: Function,
        allow_failure: bool,
    ) -> Self {
        Self {
            target,
            params,
            allow_failure,
            value: U256::ZERO,
            decoder: function,
        }
    }

    /// Set whether the call should be allowed to fail or not.
    pub const fn allow_failure(mut self, allow_failure: bool) -> Self {
        self.allow_failure = allow_failure;
        self
    }

    /// Set the value to send with the call.
    pub const fn value(mut self, value: U256) -> Self {
        self.value = value;
        self
    }
}

#[cfg(test)]
mod tests {
    use alloy::{primitives::address, sol};
    use alloy_provider::ProviderBuilder;

    use super::*;

    sol! {
        #[derive(Debug, PartialEq)]
        #[sol(rpc, abi)]
        interface ERC20 {
            function totalSupply() external view returns (uint256 totalSupply);
            function balanceOf(address owner) external view returns (uint256 balance);
            function transfer(address to, uint256 value) external returns (bool);
        }
    }

    const FORK_URL: &str = "https://reth-ethereum.ithaca.xyz/rpc";

    #[tokio::test]
    async fn test_dynamic_multicaller() {
        let _ = tracing_subscriber::fmt::try_init();

        let weth = address!("C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");
        let provider = ProviderBuilder::new().connect_anvil_with_config(|a| a.fork(FORK_URL));

        let total_supply_function = ERC20::abi::functions()
            .get("totalSupply")
            .cloned()
            .unwrap()
            .first()
            .unwrap()
            .clone();

        let balance_of_function = ERC20::abi::functions()
            .get("balanceOf")
            .cloned()
            .unwrap()
            .first()
            .unwrap()
            .clone();

        let balance_of_call_item = DynCallItem::new(
            weth,
            vec![DynSolValue::Address(address!(
                "d8dA6BF26964aF9D7eEd9e03E53415D37aA96045"
            ))],
            balance_of_function,
            false,
        );

        let total_supply_call_item =
            DynCallItem::new(weth, Vec::new(), total_supply_function, false);

        let dynamic_multicall = DynamicMulticallBuilder::new(provider.clone())
            .add_call(balance_of_call_item)
            .add_call(total_supply_call_item);

        assert_eq!(dynamic_multicall.len(), 2);

        let res = dynamic_multicall.aggregate3().await.unwrap();

        assert_eq!(res.len(), 2);

        for result in res {
            let decoded = result.unwrap();
            assert_eq!(decoded.len(), 1);
        }
    }
}
