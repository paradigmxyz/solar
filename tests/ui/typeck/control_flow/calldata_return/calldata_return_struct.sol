// ported-from: test/libsolidity/syntaxTests/controlFlow/calldataReturn/calldata_return_struct.sol
contract C {
    struct S { uint256 x; }
    function f() internal returns (S calldata) {} //~ ERROR: this variable is of calldata pointer type and can be returned
}
