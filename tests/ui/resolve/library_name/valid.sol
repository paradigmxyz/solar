//@ check-pass
// A library name is only invalid where it would name a type or an overridden contract.

library L {
    struct S {
        uint256 x;
    }

    function f(uint256 x) internal pure returns (uint256) {
        return x;
    }

    function g() external pure {}
}

using L for uint256;

contract C {
    using {L.f} for uint256;

    L.S s;

    function h(uint256 a) public view returns (uint256, string memory, bytes4) {
        return (L.f(a) + a.f() + s.x, type(L).name, L.g.selector);
    }
}
