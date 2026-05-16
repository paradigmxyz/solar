//@compile-flags: -Ztypeck

contract C {
    function f() public returns (uint256 x, uint256 y) {
        assembly {
            x := linkersymbol("file.sol:Library")
            x := memoryguard(0x80)
            x := datasize("runtime")
            y := dataoffset("runtime")
            datacopy(0, dataoffset("runtime"), datasize("runtime"))
            setimmutable(0, "immutable_id", x)
            y := loadimmutable("immutable_id")
            y := auxdataloadn(0)
            x := eofcreate("runtime", 0, 0, 0, 0)
            returncontract("runtime", 0, 0)
            x := verbatim_0i_1o(hex"58")
            y := verbatim_1i_1o(hex"6001", x)
            verbatim_2i_0o(hex"5050", x, y)
        }
    }
}
