//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/constructor/payable_new.sol
// ported-from: test/libsolidity/syntaxTests/constructor/nonpayable_new.sol

contract PayableA1 {}
contract PayableB1 is PayableA1 {
    constructor() payable {}
}

contract PayableA2 {
    constructor() {}
}
contract PayableB2 is PayableA2 {
    constructor() payable {}
}

contract PayableB3 {
    constructor() payable {}
}

contract PayableCreator {
    function f() public payable {
        new PayableB1{value: 10}();
        new PayableB2{value: 10}();
        new PayableB3{value: 10}();
    }
}

contract NonPayableA1 {
    constructor() {}
}
contract NonPayableB1 is NonPayableA1 {}

contract NonPayableA2 {
    constructor() payable {}
}
contract NonPayableB2 is NonPayableA2 {}

contract NonPayableB3 {}

contract NonPayableB4 {
    constructor() {}
}

contract NonPayableCreator {
    function f() public payable {
        new NonPayableB1{value: 10}(); //~ ERROR: cannot set option `value`
        new NonPayableB2{value: 10}(); //~ ERROR: cannot set option `value`
        new NonPayableB3{value: 10}(); //~ ERROR: cannot set option `value`
        new NonPayableB4{value: 10}(); //~ ERROR: cannot set option `value`
    }
}
