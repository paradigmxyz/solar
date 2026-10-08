// ported-from: test/libsolidity/syntaxTests/controlFlow/mappingReturn/unnamed_err.sol
contract C {
    function f() internal pure returns (mapping(uint=>uint) storage) {} //~ ERROR: this variable is of storage pointer type and can be returned
}
