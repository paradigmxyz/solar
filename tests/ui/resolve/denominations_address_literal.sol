contract C {
    struct S {
        uint256[0x1234567890123456789012345678901234567890 wei] a; //~ ERROR: cannot be used with unit denominations
    }

    uint256[0x1234567890123456789012345678901234567890 wei] a; //~ ERROR: cannot be used with unit denominations

    function f() public pure returns (uint256) {
        return 0xdCad3a6d3569DF655070DEd06cb7A1b2Ccd1D3AF ether; //~ ERROR: cannot be used with unit denominations
    }
}
