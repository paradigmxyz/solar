// Finding 40: five more interface and library declaration checks solc applies are missing or
// carry the wrong code. solc rejects every declaration below (and warns on `virtual`); we
// compile the file, reporting only 7801 with a library-function message for the modifier.
//   solc --bin symbolic-audit/interface_member_checks.sol
//   target/debug/solar --emit abi symbolic-audit/interface_member_checks.sol
interface I {
    constructor() {}                  // solc 6482 (and 4726 for the body)
    function f() external {}          // solc 4726
    modifier m() { _; }               // solc 6408
    uint256 public x;                 // solc 8274
    function g() external virtual;    // solc warning 5815
}
library L {
    modifier n() virtual { _; }       // solc 3275; we report 7801 "library functions cannot be `virtual`"
}
