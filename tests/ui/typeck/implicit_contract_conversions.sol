//@compile-flags: -Ztypeck
pragma solidity ^0.8.0;

contract Base {}
contract Unrelated {}
contract Derived is Base {}
contract MoreDerived is Derived {}

contract Test {
    function testUnrelated(Unrelated u) public { //~ WARN: function state mutability can be restricted to pure
        Base b1 = u;  //~ ERROR: mismatched types
        Base b2 = Base(u); //~ ERROR: invalid explicit type conversion
    }

    function testDerived(Derived d) public pure {
        Base b = d;  // ok - implicit conversion
        Base b2 = Base(d); // ok - explicit conversion
    }

    function testMoreDerived(MoreDerived md) public pure {
        Base b = md;  // ok - implicit conversion
        Base b2 = Base(md); // ok - explicit conversion
        Derived d1 = md; // ok - explicit conversion
        Derived d2 = Derived(md); // ok - explicit conversion
        Unrelated u1 = md; //~ ERROR: mismatched types
        Unrelated u2 = Unrelated(md); //~ ERROR: invalid explicit type conversion
    }
}