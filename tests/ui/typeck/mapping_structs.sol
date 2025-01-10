struct S {
    mapping(uint => uint) x;
}

struct Nested {
    S s;
}

function ff1(S memory) {}
function ff2(S storage) {}
function ff3() returns(S memory) {}
function ff4() returns(S storage) {}

function fpn1(Nested memory) {}
function fpn2(Nested storage) {}
function fpn3() returns(Nested memory) {}
function fpn4() returns(Nested storage) {}

contract C {
    function f1(S memory) public {}            //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    function f2(S storage) public {}           //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f3() public returns(S memory) {}  //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    function f4() public returns(S storage) {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location

    modifier m1(S memory) { _; }
    modifier m2(S storage) { _; }

    function fp1(S memory) internal {}
    function fp2(S storage) internal {}
    function fp3() internal returns(S memory) {}
    function fp4() internal returns(S storage) {}

    function n1(Nested memory) public {}            //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    function n2(Nested storage) public {}           //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function n3() public returns(Nested memory) {}  //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    function n4() public returns(Nested storage) {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location

    modifier mn1(Nested memory) { _; }
    modifier mn2(Nested storage) { _; }

    function pn1(Nested memory) internal {}
    function pn2(Nested storage) internal {}
    function pn3() internal returns(Nested memory) {}
    function pn4() internal returns(Nested storage) {}
}
