contract C {
    uint public x = 1;
    function f() public returns(uint y) {
        y = this.x();
    }
}

contract D is C {
    function g() public returns(uint z) {
        z = this.f() + 1;
    }
}
