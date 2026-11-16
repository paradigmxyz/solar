//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f 0x0000000000000000000000000000000000000000000000000000000000000000 => 32, 16, 8
//@ run-call: sideEffect => 4, 1
// ported-from: test/libsolidity/semanticTests/array/fixed_bytes_length_access.sol

contract C {
    bytes1 a;
    uint256 private calls;

    function f(bytes32 x) public view returns (uint256, uint256, uint256) {
        return (x.length, bytes16(uint128(2)).length, a.length + 7);
    }

    function sideEffect() external returns (uint256, uint256) {
        return (makeBytes().length, calls);
    }

    function makeBytes() private returns (bytes4) {
        calls += 1;
        return 0;
    }
}
