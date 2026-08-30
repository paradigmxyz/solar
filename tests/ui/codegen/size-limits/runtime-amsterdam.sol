//@ revisions: warn allow
//@[warn] compile-flags: --emit=bin --evm-version amsterdam
//@[allow] compile-flags: --emit=bin --evm-version amsterdam --allow=5574
//@ normalize-stdout-test: "(?s).+" -> ""
//@ normalize-stderr-test: "size is [0-9]+ bytes" -> "size is <SIZE> bytes"

import "./initcode.sol";

contract RuntimeAmsterdam { //~[warn] WARN: contract code size is
    function deployTest() external returns (address) {
        return address(new test());
    }

    function deployA() external returns (address) {
        return address(new A());
    }
}
