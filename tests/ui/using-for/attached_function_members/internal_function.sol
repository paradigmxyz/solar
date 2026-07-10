//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/libraries/internal_library_function_attached_to_internal_function_type.sol

library L {
    function double(function(uint256) internal pure returns (uint256) f, uint256 x)
        internal
        pure
        returns (uint256)
    {
        return f(x) * 2;
    }
}
contract C {
    using L for function(uint256) internal pure returns (uint256);

    function identity(uint256 x) internal pure returns (uint256) {
        return x;
    }

    function test(uint256 value) public returns (uint256) {
        return identity.double(value);
    }
}
