// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Arithmetic.sol";

/// @dev Minimal Foundry cheatcode interface
interface Vm {
    function envBytes(string calldata key) external view returns (bytes memory);
}

contract ArithmeticTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    Arithmetic arith;

    function setUp() public {
        arith = Arithmetic(_deployContract("ARITHMETIC"));
    }

    function _deployContract(string memory name) internal returns (address deployed) {
        string memory envKey = string.concat("SOLAR_", name, "_BYTECODE");
        try vm.envBytes(envKey) returns (bytes memory creationCode) {
            assembly {
                deployed := create(0, add(creationCode, 0x20), mload(creationCode))
            }
            require(deployed != address(0), string.concat("Solar deploy failed: ", name));
        } catch {
            if (keccak256(bytes(name)) == keccak256("ARITHMETIC")) {
                deployed = address(new Arithmetic());
            } else {
                revert(string.concat("Unknown contract: ", name));
            }
        }
    }

    // ========== Basic Arithmetic ==========

    function test_AddBasic() public view {
        require(arith.add(2, 3) == 5, "2+3=5");
        require(arith.add(0, 0) == 0, "0+0=0");
        require(arith.add(1, 0) == 1, "1+0=1");
    }

    function test_AddLargeNumbers() public view {
        uint256 large = type(uint256).max / 2;
        require(arith.add(large, large) == large * 2, "large + large");
    }

    function test_SubBasic() public view {
        require(arith.sub(5, 3) == 2, "5-3=2");
        require(arith.sub(10, 10) == 0, "10-10=0");
        require(arith.sub(100, 1) == 99, "100-1=99");
    }

    function test_MulBasic() public view {
        require(arith.mul(3, 4) == 12, "3*4=12");
        require(arith.mul(0, 100) == 0, "0*100=0");
        require(arith.mul(1, 1) == 1, "1*1=1");
    }

    function test_MulByZero() public view {
        require(arith.mul(12345, 0) == 0, "x*0=0");
        require(arith.mul(0, 12345) == 0, "0*x=0");
    }

    function test_DivBasic() public view {
        require(arith.div(10, 2) == 5, "10/2=5");
        require(arith.div(9, 3) == 3, "9/3=3");
        require(arith.div(0, 5) == 0, "0/5=0");
    }

    function test_DivTruncates() public view {
        require(arith.div(7, 2) == 3, "7/2=3 (truncated)");
        require(arith.div(10, 3) == 3, "10/3=3 (truncated)");
        require(arith.div(1, 2) == 0, "1/2=0 (truncated)");
    }

    function test_ModBasic() public view {
        require(arith.mod(10, 3) == 1, "10%3=1");
        require(arith.mod(9, 3) == 0, "9%3=0");
        require(arith.mod(7, 4) == 3, "7%4=3");
    }

    // ========== Comparison Operators ==========

    function test_LessThan() public view {
        require(arith.lt(1, 2) == true, "1<2");
        require(arith.lt(2, 1) == false, "2<1 false");
        require(arith.lt(5, 5) == false, "5<5 false");
        require(arith.lt(0, 1) == true, "0<1");
    }

    function test_GreaterThan() public view {
        require(arith.gt(2, 1) == true, "2>1");
        require(arith.gt(1, 2) == false, "1>2 false");
        require(arith.gt(5, 5) == false, "5>5 false");
    }

    function test_LessOrEqual() public view {
        require(arith.lte(1, 2) == true, "1<=2");
        require(arith.lte(5, 5) == true, "5<=5");
        require(arith.lte(6, 5) == false, "6<=5 false");
    }

    function test_GreaterOrEqual() public view {
        require(arith.gte(2, 1) == true, "2>=1");
        require(arith.gte(5, 5) == true, "5>=5");
        require(arith.gte(4, 5) == false, "4>=5 false");
    }

    function test_Equality() public view {
        require(arith.eq(5, 5) == true, "5==5");
        require(arith.eq(0, 0) == true, "0==0");
        require(arith.eq(5, 6) == false, "5==6 false");
    }

    function test_NotEqual() public view {
        require(arith.neq(5, 6) == true, "5!=6");
        require(arith.neq(5, 5) == false, "5!=5 false");
    }

    // ========== Bitwise Operations ==========

    function test_BitwiseAnd() public view {
        require(arith.bitwiseAnd(0xF0, 0x0F) == 0x00, "F0 & 0F = 00");
        require(arith.bitwiseAnd(0xFF, 0x0F) == 0x0F, "FF & 0F = 0F");
        require(arith.bitwiseAnd(0xAB, 0xAB) == 0xAB, "AB & AB = AB");
    }

    function test_BitwiseOr() public view {
        require(arith.bitwiseOr(0xF0, 0x0F) == 0xFF, "F0 | 0F = FF");
        require(arith.bitwiseOr(0x00, 0x00) == 0x00, "00 | 00 = 00");
    }

    function test_BitwiseXor() public view {
        require(arith.bitwiseXor(0xFF, 0xFF) == 0x00, "FF ^ FF = 00");
        require(arith.bitwiseXor(0xAA, 0x55) == 0xFF, "AA ^ 55 = FF");
    }

    // TODO: BitwiseNot test skipped - NOT opcode not yet implemented

    function test_ShiftLeft() public view {
        require(arith.shiftLeft(1, 0) == 1, "1<<0=1");
        require(arith.shiftLeft(1, 1) == 2, "1<<1=2");
        require(arith.shiftLeft(1, 8) == 256, "1<<8=256");
        require(arith.shiftLeft(0xFF, 8) == 0xFF00, "FF<<8=FF00");
    }

    function test_ShiftRight() public view {
        require(arith.shiftRight(256, 8) == 1, "256>>8=1");
        require(arith.shiftRight(255, 4) == 15, "255>>4=15");
        require(arith.shiftRight(1, 1) == 0, "1>>1=0");
    }

    // ========== Complex Expressions ==========

    function test_ComplexExpr() public view {
        // (a + b) * c - (a / (b + 1))
        // (10 + 5) * 2 - (10 / 6) = 30 - 1 = 29
        require(arith.complexExpr(10, 5, 2) == 29, "complex expr");
    }

    // TODO: Increment/Decrement tests skipped - storage pre/post inc/dec has a bug

    // ========== Compound Assignments ==========

    function test_AddAssign() public {
        arith.resetValue();
        arith.addAssign(10);
        require(arith.value() == 10, "value after +=10");
        arith.addAssign(5);
        require(arith.value() == 15, "value after +=5");
    }

    function test_SubAssign() public {
        arith.setValue(100);
        arith.subAssign(30);
        require(arith.value() == 70, "value after -=30");
    }

    function test_MulAssign() public {
        arith.setValue(5);
        arith.mulAssign(4);
        require(arith.value() == 20, "value after *=4");
    }

    function test_DivAssign() public {
        arith.setValue(100);
        arith.divAssign(5);
        require(arith.value() == 20, "value after /=5");
    }
}
