//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/lvalues/functions.sol
// ported-from: test/libsolidity/syntaxTests/metaTypes/codeIsNoLValue.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/015_balance_invalid.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/delete_function_type_invalid.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/delete_external_function_type_invalid.sol
// ported-from: test/libsolidity/smtCheckerTests/array_members/push_as_lhs_2d.sol
// ported-from: test/libsolidity/smtCheckerTests/array_members/push_as_lhs_3d.sol

contract Test {
    uint256 a;
    uint256 b;
    int256 c;
    uint256[] arr;
    uint256[][] nested;
    uint256[][][] deeplyNested;
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
        nested.push().push() = 2;
        deeplyNested.push().push().push() = 3;
        data.push() = 0x01;
        arr.push() = arr.pop(); //~ ERROR: mismatched types
        arr.push(1) = arr.pop(); //~ ERROR: expression has to be an lvalue
        data.push(0x01) = arr.pop(); //~ ERROR: expression has to be an lvalue
        arr.pop() = arr.pop(); //~ ERROR: expression has to be an lvalue
    }

    function testAddressMember() external {
        address(0).balance = 7; //~ ERROR: expression has to be an lvalue
    }
}

contract FunctionLvalues {
    function f() internal {}

    function g() internal {
        g = f; //~ ERROR: expression has to be an lvalue
    }

    function h() external {}

    function i() external {
        this.i = this.h; //~ ERROR: expression has to be an lvalue
    }

    function testDelete() external {
        delete f; //~ ERROR: expression has to be an lvalue
        delete this.h; //~ ERROR: expression has to be an lvalue
    }
}

contract MetaTypeMemberLvalues {
    function f() public pure {
        type(MetaTypeMemberLvalues).creationCode = new bytes(6); //~ ERROR: expression has to be an lvalue
        type(MetaTypeMemberLvalues).runtimeCode = new bytes(6); //~ ERROR: expression has to be an lvalue
    }
}
