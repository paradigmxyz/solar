struct S {
    mapping(uint => uint) x;
}

struct Nested {
    S s;
}

function free_1(S memory) {}
function free_2(S storage) {}
function free_3() returns(S memory) {}
function free_4() returns(S storage) {}

function free_nested_1(Nested memory) {}
function free_nested_2(Nested storage) {}
function free_nested_3() returns(Nested memory) {}
function free_nested_4() returns(Nested storage) {}

contract C {
    S internal var_1;
    S public var_2; //~ ERROR: getter must return at least one value

    S[] internal var_array_1;
    S[] public var_array_2; //~ ERROR: getter must return at least one value

    Nested internal var_nested_1;
    Nested public var_nested_2; //~ ERROR: types containing mappings cannot be parameter or return types of public getter functions

    Nested[] internal var_nested_array_1;
    Nested[] public var_nested_array_2; //~ ERROR: types containing mappings cannot be parameter or return types of public getter functions

    function func_1(S memory) public {}            //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    function func_2(S storage) public {}           //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function func_3() public returns(S memory) {}  //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    function func_4() public returns(S storage) {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location

    modifier mod_1(S memory) { _; }
    modifier mod_2(S storage) { _; }

    function func_internal_1(S memory) internal {}
    function func_internal_2(S storage) internal {}
    function func_internal_3() internal returns(S memory) {}
    function func_internal_4() internal returns(S storage) {}

    function func_nested_1(Nested memory) public {}            //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    function func_nested_2(Nested storage) public {}           //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location
    function func_nested_3() public returns(Nested memory) {}  //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    function func_nested_4() public returns(Nested storage) {} //~ ERROR: types containing mappings cannot be parameter or return types of public functions
    //~^ ERROR: invalid data location

    modifier mod_nested_1(Nested memory) { _; }
    modifier mod_nested_2(Nested storage) { _; }

    function func_internal_nested_1(Nested memory) internal {}
    function func_internal_nested_2(Nested storage) internal {}
    function func_internal_nested_3() internal returns(Nested memory) {}
    function func_internal_nested_4() internal returns(Nested storage) {}
}
