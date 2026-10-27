//@ compile-flags: -j8

struct S {
    mapping(uint => uint) x;
}

contract C {
    function f1(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f2(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f3(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f4(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f5(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f6(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f7(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f8(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f9(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f10(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f11(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f12(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f13(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f14(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f15(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f16(S storage) public {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
}
