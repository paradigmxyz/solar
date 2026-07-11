// ported-from: test/libsolidity/syntaxTests/using/bound_calldata_parameter_not_accepting_memory.sol

library L {
    function f(bytes calldata x) internal pure returns (uint256) {
        return x.length;
    }
}

contract C {
    using L for bytes;

    function run(bytes memory x) public pure returns (uint256) {
        return x.f(); //~ ERROR: member `f` not found
    }
}
