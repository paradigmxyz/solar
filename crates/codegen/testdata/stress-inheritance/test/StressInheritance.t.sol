// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/StressInheritance.sol";

contract DeepLinearTest {
    DeepLinear linear;
    
    function setUp() public {
        linear = new DeepLinear();
    }
    
    function test_Level0() public {
        linear.set0(100);
        assert(linear.value0() == 100);
    }
    
    function test_Level1() public {
        linear.set0(1);
        linear.set1(2);
        assert(linear.sum1() == 3);
    }
    
    function test_Level2() public {
        linear.set0(1);
        linear.set1(2);
        linear.set2(3);
        assert(linear.sum2() == 6);
    }
    
    function test_Level3() public {
        linear.set0(1);
        linear.set1(2);
        linear.set2(3);
        linear.set3(4);
        assert(linear.sum3() == 10);
    }
    
    function test_Level4() public {
        linear.set0(1);
        linear.set1(2);
        linear.set2(3);
        linear.set3(4);
        linear.set4(5);
        assert(linear.sum4() == 15);
    }
    
    function test_Level5() public {
        linear.set0(1);
        linear.set1(2);
        linear.set2(3);
        linear.set3(4);
        linear.set4(5);
        linear.set5(6);
        assert(linear.sum5() == 21);
    }
    
    function test_Level6() public {
        linear.set0(1);
        linear.set1(2);
        linear.set2(3);
        linear.set3(4);
        linear.set4(5);
        linear.set5(6);
        linear.set6(7);
        assert(linear.sum6() == 28);
    }
    
    function test_Level7() public {
        linear.set0(1);
        linear.set1(2);
        linear.set2(3);
        linear.set3(4);
        linear.set4(5);
        linear.set5(6);
        linear.set6(7);
        linear.set7(8);
        assert(linear.sum7() == 36);
    }
    
    function test_SetAllAndSum() public {
        linear.setAll(10);
        // sumAll = 10 * 9 = 90
        assert(linear.sumAll() == 90);
    }
    
    function test_Level8() public {
        linear.setAll(5);
        linear.set8(100);
        // sum = 5*8 + 100 = 140
        assert(linear.sumAll() == 140);
    }
    
    function test_GetAllValues() public {
        linear.setAll(7);
        (uint256 v0, uint256 v1, uint256 v2, uint256 v3, uint256 v4, uint256 v5, uint256 v6, uint256 v7, uint256 v8) = linear.getAllValues();
        assert(v0 == 7 && v1 == 7 && v2 == 7 && v3 == 7 && v4 == 7 && v5 == 7 && v6 == 7 && v7 == 7 && v8 == 7);
    }
}

contract DiamondMergeTest {
    DiamondMerge diamond;
    
    function setUp() public {
        diamond = new DiamondMerge();
    }
    
    function test_SetValueA() public {
        diamond.setValueA(10);
        // Override adds 1
        assert(diamond.getValueA() == 11);
    }
    
    function test_SetValueB() public {
        diamond.setValueB(10);
        assert(diamond.getValueB() == 10);
    }
    
    function test_SetValueAB() public {
        diamond.setValueAB(50);
        assert(diamond.getValueAB() == 50);
    }
    
    function test_SetExtendedA() public {
        diamond.setExtendedA(25);
        assert(diamond.getExtendedA() == 25);
    }
    
    function test_SetFinalValue() public {
        diamond.setFinalValue(100);
        assert(diamond.finalValue() == 100);
    }
    
    function test_SetAll() public {
        diamond.setAll(10);
        // valueA = 10 + 1 = 11 (override)
        // valueB = 10
        // valueAB = 10
        // extendedA = 10
        // finalValue = 10
        // sum = 11 + 10 + 10 + 10 + 10 = 51
        assert(diamond.sumAll() == 51);
    }
    
    function test_SumAB() public {
        diamond.setValueA(5);  // becomes 6
        diamond.setValueB(10);
        diamond.setValueAB(20);
        uint256 sum = diamond.sumAB();
        assert(sum == 6 + 10 + 20);
    }
}

contract MultiInterfaceTokenTest {
    MultiInterfaceToken token;
    
    function setUp() public {
        token = new MultiInterfaceToken("Test Token", "TEST");
    }
    
    function test_Name() public view {
        assert(keccak256(bytes(token.name())) == keccak256(bytes("Test Token")));
    }
    
    function test_Symbol() public view {
        assert(keccak256(bytes(token.symbol())) == keccak256(bytes("TEST")));
    }
    
    function test_Decimals() public view {
        assert(token.decimals() == 18);
    }
    
    function test_OwnerIsDeployer() public view {
        assert(token.owner() == address(this));
    }
    
    function test_InitiallyNotPaused() public view {
        assert(!token.paused());
    }
    
    function test_Mint() public {
        token.mint(address(this), 1000);
        assert(token.balanceOf(address(this)) == 1000);
        assert(token.totalSupply() == 1000);
    }
    
    function test_Transfer() public {
        token.mint(address(this), 1000);
        address recipient = address(0x1234);
        token.transfer(recipient, 100);
        assert(token.balanceOf(address(this)) == 900);
        assert(token.balanceOf(recipient) == 100);
    }
    
    function test_PauseAndUnpause() public {
        token.pause();
        assert(token.paused());
        token.unpause();
        assert(!token.paused());
    }
    
    function test_TransferOwnership() public {
        address newOwner = address(0x5678);
        token.transferOwnership(newOwner);
        assert(token.owner() == newOwner);
    }
}

contract OverrideFinalTest {
    OverrideFinal o;
    
    function setUp() public {
        o = new OverrideFinal();
    }
    
    function test_Compute() public {
        uint256 result = o.compute(10);
        assert(result == 15); // 10 + 5
        assert(o.value() == 15);
    }
    
    function test_CallSuper() public {
        uint256 result = o.callSuper(10);
        // Calls Override4.compute which does x + 4
        assert(result == 14);
        assert(o.value() == 14);
    }
}

contract ConstructorFinalTest {
    ConstructorFinal c;
    
    function setUp() public {
        c = new ConstructorFinal(10, 20, 30);
    }
    
    function test_ConstructorValues() public view {
        assert(c.baseValue() == 10);
        assert(c.middleValue() == 20);
        assert(c.finalValue() == 30);
    }
    
    function test_SumValues() public view {
        assert(c.sumValues() == 60);
    }
}
