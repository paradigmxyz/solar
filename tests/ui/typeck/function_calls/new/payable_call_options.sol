//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/constructor/payable_new.sol

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
