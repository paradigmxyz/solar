struct Custom {
    int256 f1;
}

contract A {
    function f(Custom memory custom) public returns (int256) {
        return custom.f1;
    }
}

contract B {
    uint256 public x;

    constructor(uint256 a) payable {
        x = a;
    }
}

contract C {
    function create() public {
        B b = new B{value: 1}(2);
        b = (new B{value: 1})(2);
        b = (new B){value: 1}(2);
    }
}

contract D {
    uint256 index;

    function g() public {
        (uint256 x,, uint256 y) = (7, true, 2);
        (x, y) = (y, x);
        (index,,) = (7, true, 2);
    }
}
