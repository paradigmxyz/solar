contract C {
    function f() public returns (uint256 x, uint256 y) {
        assembly {
            x := linkersymbol("file.sol:Library") //~ ERROR: unresolved symbol
            x := memoryguard(0x80) //~ ERROR: unresolved symbol
            x := datasize("runtime") //~ ERROR: unresolved symbol
            y := dataoffset("runtime") //~ ERROR: unresolved symbol
            datacopy(0, 0, 0) //~ ERROR: unresolved symbol
            setimmutable(0, "immutable_id", x) //~ ERROR: unresolved symbol
            y := loadimmutable("immutable_id") //~ ERROR: unresolved symbol
            y := auxdataloadn(0) //~ ERROR: unresolved symbol
            x := eofcreate("runtime", 0, 0, 0, 0) //~ ERROR: unresolved symbol
            returncontract("runtime", 0, 0) //~ ERROR: unresolved symbol
            x := verbatim_0i_1o(hex"58") //~ ERROR: unsupported verbatim builtin
            y := verbatim_1i_1o(hex"6001", x) //~ ERROR: unsupported verbatim builtin
            verbatim_2i_0o(hex"5050", x, y) //~ ERROR: unsupported verbatim builtin
        }
    }
}
