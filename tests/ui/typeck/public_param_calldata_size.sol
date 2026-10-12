// Each head in the ABI encoding of a variable that crosses the ABI must stay below 2^32 - 1
// bytes: calldata variables, parameters of constructors, events, errors and external function
// types, and parameters and returns of public and external functions. A struct element counts
// its static array members in full. This is stricter than solc, which checks only constructor
// and public callable parameters against calldata. Library storage parameters cross the ABI as
// slots and are not checked.
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

    function extMem(S[134217727] memory s) external pure { //~ ERROR: type too large for calldata
        s;
    }

    function intMem(S[134217727] memory s) internal pure {
        s;
    }

    function pubRet() public pure returns (S[134217727] memory s) {} //~ ERROR: type too large for calldata

    function pubOk(One[134217727] memory s) public pure {
        s;
    }

    function pubNested(uint256[2][67108864] memory s) public pure { //~ ERROR: type too large for memory
        s;
    }

    function fnParam(function(S[134217727] memory) external f) public pure { //~ ERROR: type too large for calldata
        f;
    }

    function fnParamInternal(function(S[134217727] memory) internal f) internal pure {
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

contract StorageCtor {
    constructor(uint256[2 ** 40] storage x) { //~ ERROR: this parameter has a type that can only be used internally
        x;
    }
}

library L {
    struct S {
        uint256[2] a;
    }

    function pubLib(S[134217727] memory s) public pure { //~ ERROR: type too large for calldata
        s;
    }

    function pubStorage(uint256[2 ** 40] storage x) public view returns (uint256) {
        return x[0];
    }

    function extStorage(S[134217727] storage s) external view returns (uint256) {
        return s[0].a[0];
    }
}
