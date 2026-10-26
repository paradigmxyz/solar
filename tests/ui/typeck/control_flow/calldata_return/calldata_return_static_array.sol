// ported-from: test/libsolidity/syntaxTests/controlFlow/calldataReturn/calldata_return_static_array.sol
contract C {
    function f() internal returns (uint256[1] calldata) {} //~ ERROR: this variable is of calldata pointer type and can be returned
}
