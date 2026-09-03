// With the optimizer disabled (Standard JSON `optimizer.enabled=false`),
// `first(false)` returns the derived memory address (0x120) instead of the
// stored result (8). solc returns 8. Passes with the optimizer enabled.
// Source: tests/ui/codegen/lowering/stack_only_rematerialization.sol
contract StackRematerializationUnoptimized {
    function first(bool selectFirst) external pure returns (uint256) {
        return storeAtDerivedAddress(0x100, selectFirst);
    }

    function second(bool selectFirst) external pure returns (uint256) {
        return storeAtDerivedAddress(0x120, selectFirst);
    }

    // `base` has no frame home under the direct stack-argument convention. The derived address
    // crosses the branch and may be rematerialized at the store, so spill planning must retain the
    // address rather than attempting to rebuild it after consuming `base` at the original add.
    function storeAtDerivedAddress(uint256 base, bool selectFirst)
        internal
        pure
        returns (uint256 result)
    {
        uint256 address_;
        assembly {
            address_ := add(base, 32)
        }
        if (selectFirst) {
            result = 7;
        } else {
            result = 8;
        }
        assembly {
            mstore(address_, result)
            result := mload(address_)
        }
    }
}
