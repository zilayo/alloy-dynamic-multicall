# Alloy Dynamic Multicall

A minimal, dynamic alternative to `alloy::providers::MulticallBuilder` to allow batching EVM calls via Multicall3 using runtime-known ABI functions and arguments.

Successful results are decoded into a `Vec<DynSolValue>`, allowing downstream logic to handle the dynamic types returned from each call.

## Use Cases

- 🧠 ABI is loaded dynamically (e.g. from JSON)
- 🛠 You want to batch arbitrary function calls where the call and return types are not known at compile time.

## Features

- ✅ Based on [Alloy](https://github.com/alloy-rs/alloy)'s `MulticallBuilder`.
- ✅ Uses Multicall3's `aggregate3` for efficient batching
- ✅ Decodes return values as `Vec<DynSolValue>` using runtime `Function` definitions.

## Example

```rust
use alloy_dynamic_multicall::{DynamicMulticallBuilder, DynCallItem};
use alloy::dyn_abi::DynSolValue;
use alloy::json_abi::Function;
use alloy::primitives::{U256, Address};
use alloy::providers::ProviderBuilder;

let weth: Address = "C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse().unwrap();
let provider = ProviderBuilder::new().connect_anvil();

let total_supply_fn: Function = load_fn("totalSupply");
let balance_of_fn: Function = load_fn("balanceOf");

let balance_of = DynCallItem::new(
    weth,
    vec![DynSolValue::Address("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045".parse().unwrap())],
    balance_of_fn,
);

let total_supply = DynCallItem::new(weth, vec![], total_supply_fn);

let multicall = DynamicMulticallBuilder::new(provider)
    .add_call(balance_of)
    .add_call(total_supply);

let result: Vec<Result<Vec<DynSolValue>, Failure>> = multicall.aggregate3().await.unwrap();

for call in result {
    let values = call.unwrap();
    println!("Decoded return values: {:?}", values);

    // Further downstream logic to handle the Vec<DynSolValue>
    // e.g. pattern matching to extract inner types like Address, U256, etc.
}
```
