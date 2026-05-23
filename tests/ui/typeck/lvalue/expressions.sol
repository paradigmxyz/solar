//@compile-flags: -Ztypeck

contract Test {
    uint256 a;
    uint256 b;
    int256 c;
    uint256[] arr;
    bytes data;

    function testBinaryOp() external {
        (a + b) = a; //~ ERROR: expression has to be an lvalue
    }

    function testTernary() external {
        (true ? a : b) = a; //~ ERROR: expression has to be an lvalue
    }

    function testUnary() external {
        (-c) = c; //~ ERROR: expression has to be an lvalue
    }

    function retArr() internal returns (uint256[] storage r) {
        r = arr;
    }

    function testCall() external {
        retArr() = arr; //~ ERROR: expression has to be an lvalue
        retArr()[0] = 1;
        retArr().push() = 1;
        retArr().push(1) = arr.pop(); //~ ERROR: expression has to be an lvalue
    }

    function testCallLvalue() external {
        arr.push() = 1;
        data.push() = 0x01;
        arr.push() = arr.pop(); //~ ERROR: mismatched types
        arr.push(1) = arr.pop(); //~ ERROR: expression has to be an lvalue
        data.push(0x01) = arr.pop(); //~ ERROR: expression has to be an lvalue
        arr.pop() = arr.pop(); //~ ERROR: expression has to be an lvalue
    }
}
