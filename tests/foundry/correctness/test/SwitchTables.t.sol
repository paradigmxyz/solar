// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SwitchTables {
    uint256 public value;

    function dense(uint256 key) external pure returns (uint256 result) {
        assembly {
            switch key
            case 10 { result := 100 }
            case 11 { result := 101 }
            case 12 { result := 102 }
            case 13 { result := 103 }
            case 14 { result := 104 }
            case 15 { result := 105 }
            case 16 { result := 106 }
            case 17 { result := 107 }
            default { result := 999 }
        }
    }

    function f00() external {
        value = 0;
    }

    function f01() external {
        value = 1;
    }

    function f02() external {
        value = 2;
    }

    function f03() external {
        value = 3;
    }

    function f04() external {
        value = 4;
    }

    function f05() external {
        value = 5;
    }

    function f06() external {
        value = 6;
    }

    function f07() external {
        value = 7;
    }

    function f08() external {
        value = 8;
    }

    function f09() external {
        value = 9;
    }

    function f10() external {
        value = 10;
    }

    function f11() external {
        value = 11;
    }

    function f12() external {
        value = 12;
    }

    function f13() external {
        value = 13;
    }

    function f14() external {
        value = 14;
    }

    function f15() external {
        value = 15;
    }

    function f16() external {
        value = 16;
    }

    function f17() external {
        value = 17;
    }

    function f18() external {
        value = 18;
    }

    function f19() external {
        value = 19;
    }

    function f20() external {
        value = 20;
    }

    function f21() external {
        value = 21;
    }

    function f22() external {
        value = 22;
    }

    function f23() external {
        value = 23;
    }

    function f24() external {
        value = 24;
    }

    function f25() external {
        value = 25;
    }

    function f26() external {
        value = 26;
    }

    function f27() external {
        value = 27;
    }

    function f28() external {
        value = 28;
    }

    function f29() external {
        value = 29;
    }

    function f30() external {
        value = 30;
    }

    function f31() external {
        value = 31;
    }
}

contract SwitchTablesTest {
    SwitchTables tables;

    function setUp() public {
        tables = new SwitchTables();
    }

    function testBucketDispatch() public {
        tables.f00();
        assert(tables.value() == 0);
        tables.f07();
        assert(tables.value() == 7);
        tables.f31();
        assert(tables.value() == 31);
    }

    function testBucketDispatchMiss() public {
        (bool success,) = address(tables).call(hex"ffffffff");
        assert(!success);
    }

    function testDenseSwitch() public view {
        assert(tables.dense(9) == 999);
        assert(tables.dense(10) == 100);
        assert(tables.dense(14) == 104);
        assert(tables.dense(17) == 107);
        assert(tables.dense(18) == 999);
    }
}
