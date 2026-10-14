contract C {
    function f() public pure returns (uint r, uint s) {
        assembly {
            r := 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
            s := 0x10000000000000000000000000000000000000000000000000000000000000000 //~ ERROR: number literal too large (> 256 bits)
        }
    }
}
