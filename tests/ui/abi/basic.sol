//@ compile-flags: --emit=abi,hashes --pretty-json
//@ filecheck:

// CHECK-LABEL: "ROOT/tests/ui/abi/basic.sol:C":
// CHECK: "type": "constructor"
// CHECK: "name": "Er"
// CHECK: "name": "Ev"
// CHECK: "name": "f8"
// CHECK: "type": "receive"
// CHECK: "hashes": {
// CHECK-LABEL: "ROOT/tests/ui/abi/basic.sol:D":
// CHECK: "type": "constructor"
// CHECK: "stateMutability": "payable"
// CHECK: "name": "f8"

struct S1 {
    uint x;
    string[] y;
    bool[2] z;
}

contract C {
    struct S2 {
        uint x;
        string[] y;
        bool[2] z;
    }

    constructor() {}
    fallback() external {}
    receive() external payable {}

    type UDVT is uint256;

    event Ev(uint a, uint indexed b, bool[] c, string x, UDVT u, UDVT indexed u2);
    error Er(uint a, bool[] c, string x, UDVT u);

    function f1() public {}
    function f2() external {}
    function f3() public view {}
    function f4() public pure {}
    function f5() public payable {}

    function f6() public returns(uint a, bool[] memory c, string[3] memory x, UDVT u, S1 memory $s, S2[][69][] memory s) {}
    function f7(uint a, bool[] memory c, string[3] memory x, UDVT u, S1 memory $s, S2[][69][] memory s) public {}
    function f8(uint a, bool[] memory c, string[3] memory x, UDVT u, S1 memory $s, S2[][69][] memory s) public returns(uint a1, bool[] memory c1, string[3] memory x1, UDVT u1, S1 memory $s_, S2[][69][] memory s1) {}
}

contract D is C {
    constructor(uint a, bool[] memory c, string[3] memory x, UDVT u, S1 memory $s, S2[][69][] memory s) payable {}
}
