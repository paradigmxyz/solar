// source

event E1(); //~ ERROR: event with same name and parameter types declared twice
event E1();

event E2(uint); //~ ERROR: event with same name and parameter types declared twice
event E2(uint);

event E3(uint); //~ ERROR: event with same name and parameter types declared twice
event E3(uint) anonymous;

event E4(uint); //~ ERROR: event with same name and parameter types declared twice
event E4(uint indexed);

function f1() public {} //~ ERROR: function with same name and parameter types declared twice
function f1() public {}

function f2() public {} //~ ERROR: function with same name and parameter types declared twice
function f2() public {}
function f2() public {}

function f2_2() public {}
function f2_2(int) public {}
function f2_2(uint) public {} //~ ERROR: function with same name and parameter types declared twice
function f2_2(uint) public {}

function f3(int) public {}
function f3(uint) public {}

function f4(int) public {}
function f4(int, int) public {}

function f5(int) public {} //~ ERROR: function with same name and parameter types declared twice
function f5(int) public {}

function f6(string memory) public {} //~ ERROR: function with same name and parameter types declared twice
function f6(string calldata) public {}

function f7(string storage) internal {}
function f7(string memory) public {}

// function f8(string transient) internal {}
// function f8(string storage) public {}

// function f9(string calldata) public {}
// function f9(string transient) internal {}

// contracts

contract C {
    event E1(); //~ ERROR: event with same name and parameter types declared twice
    event E1();

    event E2(uint); //~ ERROR: event with same name and parameter types declared twice
    event E2(uint);

    event E3(uint); //~ ERROR: event with same name and parameter types declared twice
    event E3(uint) anonymous;

    event E4(uint); //~ ERROR: event with same name and parameter types declared twice
    event E4(uint indexed);

    function f1() public {} //~ ERROR: function with same name and parameter types declared twice
    function f1() public {}

    function f2() public {} //~ ERROR: function with same name and parameter types declared twice
    function f2() public {}
    function f2() public {}

    function f22() public {} //~ ERROR: function with same name and parameter types declared twice
    function f22() public {}
    function f22() public {}

    function f3(int) public {}
    function f3(uint) public {}

    function f4(int) public {}
    function f4(int, int) public {}

    function f5(int) public {} //~ ERROR: function with same name and parameter types declared twice
    function f5(int) public {}

    function f6(string memory) public {} //~ ERROR: function with same name and parameter types declared twice
    function f6(string calldata) public {}

    function f7(string storage) internal {}
    function f7(string memory) public {}

    // function f8(string transient) internal {}
    // function f8(string storage) public {}

    // function f9(string calldata) public {}
    // function f9(string transient) internal {}
}

contract C2 {
    event E5();
}

contract D is C2 {
    event E5() anonymous; //~ ERROR: event with same name and parameter types declared twice
}
