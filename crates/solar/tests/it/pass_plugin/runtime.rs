use super::*;
use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_primitives::{Address, Bytes, TxKind, U256, hex};
use evm2::{
    BaseEvmTypes, Evm, Precompiles, SpecId,
    bytecode::Bytecode,
    env::BlockEnv,
    ethereum::{TxEnvelope, ethereum_tx_registry},
    evm::{AccountInfo, InMemoryDB},
};

pub(super) fn execute(artifact: &Value, input: &[u8]) -> Vec<u8> {
    let contract = artifact["contracts"].as_object().unwrap().values().next().unwrap();
    let bytecode = hex::decode(contract["bin-runtime"].as_str().unwrap()).unwrap();
    let caller = Address::repeat_byte(0x22);
    let address = Address::repeat_byte(0x33);
    let mut database = InMemoryDB::default();
    database.insert_account_info(&caller, AccountInfo::default().with_balance(U256::MAX));
    database.insert_account_info(
        &address,
        AccountInfo::default().with_code(Bytecode::new_legacy(bytecode.into())),
    );
    let spec = SpecId::OSAKA;
    let mut evm = Evm::<BaseEvmTypes>::new(
        spec,
        BlockEnv::<BaseEvmTypes>::default(),
        ethereum_tx_registry(spec),
        database,
        Precompiles::base(spec),
    );
    let tx = Recovered::new_unchecked(
        TxEnvelope::Legacy(TxLegacy {
            nonce: 0,
            to: TxKind::Call(address),
            input: Bytes::copy_from_slice(input),
            gas_price: 0,
            value: U256::ZERO,
            chain_id: None,
            gas_limit: 10_000_000,
        }),
        caller,
    );
    let result = evm.transact(&tx).unwrap().commit();
    assert!(result.status, "EVM execution failed: {:?}", result.stop);
    result.output.to_vec()
}
