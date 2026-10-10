contract C {
    enum E { this, _ } //~ ERROR: the name `this` is reserved
    //~^ ERROR: the name `_` is reserved
    struct S { uint super; } //~ ERROR: the name `super` is reserved
    event Ev(uint this); //~ ERROR: the name `this` is reserved
    error Er(uint _); //~ ERROR: the name `_` is reserved
    error super(); //~ ERROR: the name `super` is reserved
    function(uint this) internal f; //~ ERROR: the name `this` is reserved
    //~^ WARN: named function type parameters are deprecated
    mapping(address this => uint super) m;

    function g() public {
        try C(msg.sender).h() returns (uint super) {} catch Error(string memory _) {}
        //~^ ERROR: the name `super` is reserved
        //~| ERROR: the name `_` is reserved
    }

    function h() external returns (uint) {}
}

contract D {
    modifier _() { _; } //~ ERROR: the name `_` is reserved
    function this() internal {} //~ ERROR: the name `this` is reserved
    function super() private {} //~ ERROR: the name `super` is reserved
}

contract E {
    uint public _; //~ ERROR: the name `_` is reserved
    function this() external {}
    function super() public {}
}

library L {
    function _() public {}
}

function _() {} //~ ERROR: the name `_` is reserved
contract super {} //~ ERROR: the name `super` is reserved
struct this { uint a; } //~ ERROR: the name `this` is reserved
