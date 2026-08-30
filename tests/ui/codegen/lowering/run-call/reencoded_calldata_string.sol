//@ filecheck:
// CHECK: @module
//@ compile-flags: --allow=2018
//@ codegen-matrix: standard
//@ run-call: f(string) "" => 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000
//@ run-call: test() => 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000036162630000000000000000000000000000000000000000000000000000000000
//@ run-call: encodeDynamic(string) "" => 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000
//@ run-call: encodeDynamic(string) "abc" => 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000036162630000000000000000000000000000000000000000000000000000000000
//@ run-call: encodeStatic(uint256) 7 => 0x0000000000000000000000000000000000000000000000000000000000000007
// ported-from: test/libsolidity/semanticTests/abicoder/cleanup/reencoded_calldata_string.sol

contract C {
    function f(string calldata x) external returns (bytes memory r) {
        uint256 mptr;
        assembly {
            // Dirty memory.
            mptr := mload(0x40)
            for { let i := mptr } lt(i, add(mptr, 0x0100)) { i := add(i, 32) }
            { mstore(i, sub(0, 1)) }
        }
        r = abi.encode(x);
        assembly {
            // Assert that we dirtied the memory that was encoded to.
            if iszero(eq(mptr, r)) { revert(0, 0) }
        }
    }

    function test() external returns (bytes memory) {
        return this.f("abc");
    }

    function encodeDynamic(string calldata value) external pure returns (bytes memory result) {
        uint256 mptr;
        assembly {
            mptr := mload(0x40)
        }
        result = abi.encode(value);
        assembly {
            if iszero(eq(mptr, result)) { revert(0, 0) }
        }
    }

    function encodeStatic(uint256 value) external pure returns (bytes memory result) {
        uint256 mptr;
        assembly {
            mptr := mload(0x40)
        }
        result = abi.encode(value);
        assembly {
            if iszero(eq(mptr, result)) { revert(0, 0) }
        }
    }
}
