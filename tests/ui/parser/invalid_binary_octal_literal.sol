// Invalid digits in a binary or octal literal must not panic the parser.
contract C {
    uint a = 0b2; //~ ERROR: integers in base binary are not supported
    uint b = 0b12; //~ ERROR: integers in base binary are not supported
    uint c = 0b1_2; //~ ERROR: integers in base binary are not supported
    uint d = 0o8; //~ ERROR: integers in base octal are not supported
    uint e = 0o78; //~ ERROR: integers in base octal are not supported
    uint f = 0b1; //~ ERROR: integers in base binary are not supported

    function g() public pure {
        assembly {
            let x := 0b2 //~ ERROR: integers in base binary are not supported
            let y := 0o9 //~ ERROR: integers in base octal are not supported
        }
    }
}
