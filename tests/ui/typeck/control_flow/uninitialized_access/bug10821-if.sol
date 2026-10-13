// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/bug10821-if.sol
contract Test {

  struct Sample { bool flag; }
  Sample public s;

   function testFunc() external {
        if(true){}
        Sample storage t;
        t.flag=true; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
}
