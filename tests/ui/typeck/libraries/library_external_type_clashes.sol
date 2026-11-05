// Library functions whose types share a canonical name have the same signature.

import {A as B, S as T} from "./auxiliary/library_clash_types.sol";

type A is uint;

struct S {
    uint x;
}

library L {
    function f(mapping(A => uint) storage) public {} //~ ERROR: function overload clash during conversion to external types for arguments
    function f(mapping(B => uint) storage) public {}

    function g(S storage) public {} //~ ERROR: function overload clash during conversion to external types for arguments
    function g(T storage) public {}

    function h(mapping(A => uint) storage) public {}
    function h(mapping(uint => uint) storage) public {}
}
