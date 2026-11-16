//@ codegen-matrix: standard
//@ run-call: mutateFirst 0x0000000000000000000000000000000000000000000000000000000000000001 => 1
//@ run-call: mutateSecond 0x0000000000000000000000000000000000000000000000000000000000000001 => 1
//@ run-call: mutateAlias 0x0000000000000000000000000000000000000000000000000000000000000001 => 1

contract AbiDecodeStaticStructAlias {
    struct Value {
        uint256 value;
    }

    function mutateFirst(bytes memory encoded) external pure returns (uint256) {
        Value memory value = abi.decode(encoded, (Value));
        encoded[31] = 0x02;
        return value.value;
    }

    function mutateSecond(bytes memory encoded) external pure returns (uint256) {
        Value memory value = abi.decode(encoded, (Value));
        encoded[31] = 0x03;
        return value.value;
    }

    function mutateAlias(bytes memory encoded) external pure returns (uint256) {
        return mutateAliasInner(encoded, encoded);
    }

    function mutateAliasInner(bytes memory decoded, bytes memory alias_) internal pure returns (uint256) {
        Value memory value = abi.decode(decoded, (Value));
        if (alias_.length == 0) return 0;
        alias_[31] = 0x04;
        return value.value;
    }
}
