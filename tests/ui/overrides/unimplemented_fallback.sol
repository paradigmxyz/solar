// A `fallback` or `receive` function without a body is unimplemented too.

abstract contract A {
    fallback() external virtual; //~ NOTE: unimplemented function `fallback` defined in `A`
}

abstract contract B {
    receive() external payable virtual; //~ NOTE: unimplemented function `receive` defined in `B`
}

contract C is A {} //~ ERROR: contract `C` has unimplemented functions
contract D is B {} //~ ERROR: contract `D` has unimplemented functions

contract Implemented is A {
    fallback() external override {}
}
