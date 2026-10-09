// Like solc, inline assembly numbers are plain decimal or hexadecimal integers, without
// underscores.
contract C {
    function f() public pure returns (uint256 r) {
        assembly {
            switch 0.5 //~ ERROR: invalid number literal
            case 2e1 { r := 1 } //~ ERROR: invalid number literal
            default { r := 0x10 }
            r := add(r, 1_000) //~ ERROR: invalid number literal
            r := add(r, 0x1_0) //~ ERROR: invalid number literal
            r := add(r, 0x1F)
        }
    }
}
