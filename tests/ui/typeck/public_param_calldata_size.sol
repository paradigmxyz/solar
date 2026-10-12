// Parameters of constructors, public functions, events and errors are also checked against
// calldata, where a struct element counts its static array members in full. External functions
// are checked only against their declared location, as in solc.
contract C {
    struct S {
        uint256[2] a;
    }
    struct One {
        uint256 a;
    }

    constructor(S[134217727] memory s) { //~ ERROR: type too large for calldata
        s;
    }

    function pubMem(S[134217727] memory s) public pure { //~ ERROR: type too large for calldata
        s;
    }

    function extMem(S[134217727] memory s) external pure {
        s;
    }

    function intMem(S[134217727] memory s) internal pure {
        s;
    }

    function pubRet() public pure returns (S[134217727] memory s) {}

    function pubOk(One[134217727] memory s) public pure {
        s;
    }

    function pubNested(uint256[2][67108864] memory s) public pure { //~ ERROR: type too large for memory
        s;
    }

    function fnParam(function(S[134217727] memory) external f) public pure {
        f;
    }
}

contract D {
    struct S {
        uint256[2] a;
    }

    event E(S[134217727] s); //~ ERROR: type too large for calldata
    error Err(S[134217727] s); //~ ERROR: type too large for calldata
}

abstract contract A {
    struct S {
        uint256[2] a;
    }

    constructor(S[134217727] memory s) {
        s;
    }
}

library L {
    struct S {
        uint256[2] a;
    }

    function pubLib(S[134217727] memory s) public pure { //~ ERROR: type too large for calldata
        s;
    }
}
