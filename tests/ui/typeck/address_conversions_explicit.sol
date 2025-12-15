//@compile-flags: -Ztypeck
contract C {
    function f(
        address a,
        bytes20 b20,
        uint160 u160,
        bytes32 b32,
        uint256 u256
    ) public pure {
        // Valid conversions
        bytes20 b = bytes20(a);
        address a2 = address(b20);
        uint160 u = uint160(a);
        address a3 = address(u160);
        address payable p = payable(a);
        address payable p2 = payable(p);

        // Invalid conversions
        bytes32 b32_from_a = bytes32(a); //~ ERROR: cannot convert
        address a4 = address(b32); //~ ERROR: cannot convert
        uint256 u256_from_a = uint256(a); //~ ERROR: cannot convert
        address a5 = address(u256); //~ ERROR: cannot convert
    }
}
