contract C {
    struct S {
        mapping(uint => uint) x;
    }

    function f1(S memory) public {}            //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    function f2(S storage) public {}           //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function f3() public returns(S memory) {}  //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    function f4() public returns(S storage) {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location

    function fp1(S memory) internal {}
    function fp2(S storage) internal {}
    function fp3() internal returns(S memory) {}
    function fp4() internal returns(S storage) {}
    
    struct Nested {
        S s;
    }

    function n1(Nested memory) public {}            //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    function n2(Nested storage) public {}           //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function n3() public returns(Nested memory) {}  //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    function n4() public returns(Nested storage) {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location

    function pn1(Nested memory) internal {}
    function pn2(Nested storage) internal {}
    function pn3() internal returns(Nested memory) {}
    function pn4() internal returns(Nested storage) {}
}
