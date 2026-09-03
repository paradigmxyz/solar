//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: memoryLayout => true
//@ run-call: cleanup 1 => 0, 0
// dirtyReturn(uint256) cleans the low eight padding bytes of the memory word.
//@ run-call: 0x2aae0ed63031323334353637383930313233343536373839616263645800000000000000 => 0x3031323334353637383930313233343536373839616263640000000000000000
//@ run-call: cleanupArray 1 => 0
//@ run-call: packedArray 1 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: memoryStructToStorage 1 => 0
//@ run-call: storageStructToMemory => true
//@ run-call: memoryArrayToStorage 1 => 0
//@ run-call: storageArrayToMemory => true
// ported-from: test/libsolidity/semanticTests/abicoder/cleanup/function_v2.sol

pragma abicoder v2;

contract ExternalFunctionPointerMemoryCleanup {
    struct Holder {
        function() external callback;
    }

    Holder private stored;
    function() external[] private callbacks;

    function dummy() external {}

    function raw(function() external) external pure returns (uint256 value) {
        assembly {
            value := calldataload(4)
        }
    }

    function validate(Holder calldata holder) external pure returns (uint256 value) {
        holder.callback;
        assembly {
            value := calldataload(4)
        }
    }

    function validateArray(function() external[] calldata values)
        external
        pure
        returns (uint256 value)
    {
        values[0];
        assembly {
            value := calldataload(68)
        }
    }

    function memoryLayout() external view returns (bool) {
        Holder memory holder = Holder(this.dummy);
        uint256 value;
        assembly {
            value := mload(holder)
        }
        uint256 expected =
            (uint256(uint160(address(this))) << 96) | (uint256(uint32(this.dummy.selector)) << 64);
        return value == expected;
    }

    function cleanup(uint256 value) external view returns (uint256, uint256) {
        Holder memory holder = Holder(this.dummy);
        assembly {
            mstore(holder, value)
        }
        return (this.raw(holder.callback), this.validate(holder));
    }

    function dirtyReturn(uint256 value) external view returns (Holder memory holder) {
        holder = Holder(this.dummy);
        assembly {
            mstore(holder, value)
        }
    }

    function cleanupArray(uint256 value) external view returns (uint256) {
        function() external[] memory values = new function() external[](1);
        assembly {
            mstore(add(values, 32), value)
        }
        return this.validateArray(values);
    }

    function packedArray(uint256 value) external pure returns (bytes memory) {
        function() external[] memory values = new function() external[](1);
        assembly {
            mstore(add(values, 32), value)
        }
        return abi.encodePacked(values);
    }

    function memoryStructToStorage(uint256 value) external returns (uint256) {
        Holder memory holder = Holder(this.dummy);
        assembly {
            mstore(holder, value)
        }
        stored = holder;
        return this.raw(stored.callback);
    }

    function storageStructToMemory() external returns (bool) {
        stored = Holder(this.dummy);
        Holder memory holder = stored;
        uint256 actual;
        assembly {
            actual := mload(holder)
        }
        uint256 expected =
            (uint256(uint160(address(this))) << 96) | (uint256(uint32(this.dummy.selector)) << 64);
        return actual == expected;
    }

    function memoryArrayToStorage(uint256 value) external returns (uint256) {
        function() external[] memory values = new function() external[](1);
        assembly {
            mstore(add(values, 32), value)
        }
        callbacks = values;
        return this.raw(callbacks[0]);
    }

    function storageArrayToMemory() external returns (bool) {
        callbacks.push(this.dummy);
        function() external[] memory values = callbacks;
        uint256 actual;
        assembly {
            actual := mload(add(values, 32))
        }
        uint256 expected =
            (uint256(uint160(address(this))) << 96) | (uint256(uint32(this.dummy.selector)) << 64);
        return actual == expected;
    }
}
