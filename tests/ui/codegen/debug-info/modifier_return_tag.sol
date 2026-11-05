//@ compile-flags: -Zmir-pipeline=none -Zdump=mir -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
// solc maps the return tag after a modifier chain to the function definition, so the jump to it
// has a location at MIR input in functions, constructors, and base constructors alike. The
// pass pipeline is empty because ABI lowering adds its own gaps, see `gap_check.sol`.
contract Base {
    uint256 public b;

    modifier base() {
        _;
        b += 1;
    }

    constructor(uint256 v) base {
        b = v;
    }
}

contract ModifierReturnTag is Base(1) {
    uint256 public x;

    modifier post() {
        _;
        x = 77;
    }

    constructor() post {
        x = 1;
    }

    function h() external post returns (uint256 r) {
        r = 5;
    }
}
