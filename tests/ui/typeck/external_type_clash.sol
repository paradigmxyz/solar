// Test case 1: Function with same name but different parameter types
contract Base1 {
    function foo(uint x) public {}
}

contract Derived1 is Base1 {
    function foo(string memory x) public {}
    //^ ERROR: Function overload clash during conversion to external types for arguments
}

// Test case 2: Public state variable with same name but different types
contract Base2 {
    uint public bar;
}

contract Derived2 is Base2 {
    string public bar;
    //^ ERROR: Function overload clash during conversion to external types for arguments
}

// Test case 3: No type clash (different names)
contract Base3 {
    function foo(uint x) public {}
}

contract Derived3 is Base3 {
    function bar(string memory x) public {}
}

// Test case 4: No type clash (same parameter types)
contract Base4 {
    function foo(uint x) public {}
}

contract Derived4 is Base4 {
    function foo(uint x) public {}
}

// Test case 5: Multiple inheritance with type clash
contract Base5A {
    function baz(uint x) public {}
}

contract Base5B {
    function baz(string memory x) public {}
}

contract Derived5 is Base5A, Base5B {
    //^ ERROR: Function overload clash during conversion to external types for arguments
}

// Test case 6: Complex parameter types
contract Base6 {
    function complex(uint[] memory x) public {}
}

contract Derived6 is Base6 {
    function complex(string[] memory x) public {}
    //^ ERROR: Function overload clash during conversion to external types for arguments
}

// Test case 7: Multiple parameters
contract Base7 {
    function multi(uint x, string memory y) public {}
}

contract Derived7 is Base7 {
    function multi(string memory x, uint y) public {}
    //^ ERROR: Function overload clash during conversion to external types for arguments
}

// Test case 8: Function and public state variable clash
contract Base8 {
    uint public value;
}

contract Derived8 is Base8 {
    function value() public returns (string memory) {}
    //^ ERROR: Function overload clash during conversion to external types for arguments
} 