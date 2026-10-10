// ported-from: test/libsolidity/syntaxTests/types/type_casting_named_parameter.sol

contract C {
    function f() public pure returns (uint256) {
        int256 x = -1;
        return uint256({value: x}); //~ ERROR: type conversions cannot take named arguments
    }
}
