// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/StressModifiers.sol";

contract StressModifiersTest {
    StressModifiers sm;
    
    function setUp() public {
        sm = new StressModifiers();
    }
    
    // ========== Single modifier tests ==========
    
    function test_SingleModifier1() public {
        sm.singleModifier1();
        assert(sm.value() == 1);
    }
    
    function test_SingleModifier2() public {
        sm.singleModifier2();
        assert(sm.value() == 2);
    }
    
    function test_SingleModifier3() public {
        sm.singleModifier3();
        assert(sm.value() == 3);
        assert(!sm.isLocked());
    }
    
    // ========== Two modifier tests ==========
    
    function test_TwoModifiers1() public {
        sm.twoModifiers1();
        assert(sm.value() == 10);
    }
    
    function test_TwoModifiers2() public {
        sm.twoModifiers2();
        assert(sm.value() == 11);
    }
    
    function test_TwoModifiers3() public {
        sm.twoModifiers3();
        assert(sm.value() == 12);
    }
    
    function test_TwoModifiers4() public {
        sm.twoModifiers4();
        assert(sm.value() == 13);
    }
    
    // ========== Three modifier tests ==========
    
    function test_ThreeModifiers1() public {
        sm.threeModifiers1();
        assert(sm.value() == 20);
    }
    
    function test_ThreeModifiers2() public {
        sm.threeModifiers2();
        assert(sm.value() == 21);
    }
    
    function test_ThreeModifiers3() public {
        sm.threeModifiers3();
        assert(sm.value() == 22);
        assert(sm.callCount() == 1);
    }
    
    // ========== Four modifier tests ==========
    
    function test_FourModifiers1() public {
        sm.fourModifiers1();
        assert(sm.value() == 30);
    }
    
    function test_FourModifiers2() public {
        sm.fourModifiers2();
        assert(sm.value() == 31);
        assert(sm.callCount() == 1);
    }
    
    // ========== Five modifier tests ==========
    
    function test_FiveModifiers1() public {
        sm.fiveModifiers1();
        assert(sm.value() == 40);
        assert(sm.callCount() == 1);
    }
    
    function test_FiveModifiers2() public {
        sm.fiveModifiers2();
        assert(sm.value() == 41);
        assert(sm.beforeCount() == 1);
        assert(sm.afterCount() == 1);
    }
    
    // ========== Six modifier tests ==========
    
    function test_SixModifiers() public {
        sm.sixModifiers();
        assert(sm.value() == 50);
        assert(sm.beforeCount() == 1);
        assert(sm.afterCount() == 1);
    }
    
    // ========== Parameter modifier tests ==========
    
    function test_WithParamModifiers1() public {
        sm.withParamModifiers1(50);
        assert(sm.value() == 50);
    }
    
    function test_WithParamModifiers2() public {
        sm.withParamModifiers2(500);
        assert(sm.value() == 500);
    }
    
    function test_WithParamModifiers3() public {
        sm.withParamModifiers3(100);
        assert(sm.value() == 100);
    }
    
    // ========== Counting order tests ==========
    
    function test_OrderBefore() public {
        sm.testOrderBefore();
        assert(sm.value() == 100);
        assert(sm.beforeCount() == 3);
    }
    
    function test_OrderAfter() public {
        sm.testOrderAfter();
        assert(sm.value() == 101);
        assert(sm.afterCount() == 3);
    }
    
    function test_OrderBoth() public {
        sm.testOrderBoth();
        assert(sm.value() == 102);
        assert(sm.beforeCount() == 3);
        assert(sm.afterCount() == 3);
    }
    
    function test_OrderMixed() public {
        sm.testOrderMixed();
        assert(sm.value() == 103);
        // beforeCount should have 1 from countBefore + 1 from countBoth = 2
        assert(sm.beforeCount() == 2);
        // afterCount should have 1 from countAfter + 1 from countBoth = 2
        assert(sm.afterCount() == 2);
    }
    
    // ========== NonReentrant test ==========
    
    function test_NonReentrantResets() public {
        sm.singleModifier3();
        assert(!sm.isLocked());
        
        sm.singleModifier3();
        assert(!sm.isLocked());
    }
    
    // ========== Pause/unpause tests ==========
    
    function test_PauseUnpause() public {
        assert(!sm.paused());
        
        sm.pause();
        assert(sm.paused());
        
        sm.unpause();
        assert(!sm.paused());
    }
    
    // ========== Owner/admin tests ==========
    
    function test_SetOwner() public {
        address newOwner = address(0x1234);
        sm.setOwner(newOwner);
        assert(sm.owner() == newOwner);
    }
    
    function test_SetAdmin() public {
        address newAdmin = address(0x5678);
        sm.setAdmin(newAdmin);
        assert(sm.admin() == newAdmin);
    }
    
    // ========== Whitelist tests ==========
    
    function test_Whitelist() public {
        address addr = address(0x1111);
        assert(!sm.whitelist(addr));
        
        sm.addToWhitelist(addr);
        assert(sm.whitelist(addr));
        
        sm.removeFromWhitelist(addr);
        assert(!sm.whitelist(addr));
    }
    
    // ========== Reset tests ==========
    
    function test_ResetCounts() public {
        sm.threeModifiers3(); // Increments callCount
        assert(sm.callCount() == 1);
        
        sm.resetCounts();
        assert(sm.callCount() == 0);
        assert(sm.beforeCount() == 0);
        assert(sm.afterCount() == 0);
    }
    
    function test_ResetValue() public {
        sm.singleModifier1();
        assert(sm.value() == 1);
        
        sm.resetValue();
        assert(sm.value() == 0);
    }
    
    // ========== GetCounts test ==========
    
    function test_GetCounts() public {
        sm.testOrderMixed();
        
        (uint256 call, uint256 before, uint256 after_) = sm.getCounts();
        assert(call == 0);
        assert(before == 2);
        assert(after_ == 2);
    }
}

contract ChildWithModifiersTest {
    ChildWithModifiers child;
    
    function setUp() public {
        child = new ChildWithModifiers();
    }
    
    function test_SetChildValue() public {
        child.setChildValue(42);
        assert(child.childValue() == 42);
    }
    
    function test_ComplexChild() public {
        child.complexChild(10);
        assert(child.childValue() == 10);
        assert(child.value() == 20);
        assert(child.callCount() == 1);
    }
    
    function test_SetOwnerOverride() public {
        address newOwner = address(0x9999);
        child.setOwner(newOwner);
        assert(child.owner() == newOwner);
        assert(child.childValue() == 1);
    }
}

contract InitializableContractTest {
    InitializableContract ic;
    
    function setUp() public {
        ic = new InitializableContract();
    }
    
    function test_Initialize() public {
        assert(!ic.isInitialized());
        
        ic.initialize(100);
        
        assert(ic.isInitialized());
        assert(ic.data() == 100);
        assert(ic.initOwner() == address(this));
    }
    
    function test_UpdateDataAfterInit() public {
        ic.initialize(50);
        
        ic.updateData(200);
        assert(ic.data() == 200);
    }
}
