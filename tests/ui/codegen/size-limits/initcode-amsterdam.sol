//@ revisions: warn allow
//@[warn] compile-flags: -Zdump=evm-ir --evm-version amsterdam
//@[allow] compile-flags: -Zdump=evm-ir --evm-version amsterdam --allow=3860
//@ normalize-stdout-test: "(?s).+" -> ""
//@ normalize-stderr-test: "size is [0-9]+ bytes" -> "size is <SIZE> bytes"

import "./initcode.sol";

contract D is A {
    function d() public pure returns (uint256) {
        return 1;
    }
}

contract E is A {
    function e() public pure returns (uint256) {
        return 2;
    }
}

contract InitcodeAmsterdam { //~[warn] WARN: contract initcode size is
    constructor() {
        new test();
        new A();
        new B();
        new C();
        new D();
        new E();
    }
}
