// SPDX-License-Identifier: MIT
event E(uint a);
error Er();

uint constant K = 3 /* file */;

contract C {
    uint x /* state */;
    uint constant y = 1 /* constant */ ;
    uint public z = 2 // line
    ;

    modifier m() { _ /* placeholder */; }

    function f() public {
        uint a /* local */;
        uint b = 1 /* initialized */;
        (uint c, uint d) = (1, 2) /* tuple */;
        emit E(1) /* emit */;
        for (;;) { break /* break */; }
        revert Er() /* revert */;
    }
}
