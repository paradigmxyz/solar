//@compile-flags: -Ztypeck
  pragma solidity ^0.8.0;

  contract Base {}
  contract Unrelated {}
  contract Derived is Base {}
  contract MoreDerived is Derived {}

  contract Test {
      function testFail(Unrelated u) public {
          Base b = u;  //~ ERROR: mismatched types
      }

      function testPass(Derived d) public pure {
          Base b = d;  // ok - implicit conversion
      }

      function testPassAgain(MoreDerived d) public pure {
          Base b = d;  // ok - implicit conversion
      }
  }