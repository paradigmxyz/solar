// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/bug10821-modifier.sol
contract Test {

  struct Sample { bool flag; }
  Sample public s;

  modifier checkAddr(address _a){
      require(_a!=address(0));
      _;
  }
  function testFunc(address _a) external checkAddr(_a) {
        Sample storage t;
        t.flag=true; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
}
