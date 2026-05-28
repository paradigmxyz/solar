// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Enums.sol";

contract EnumsTest {
    Enums enums;

    function setUp() public {
        enums = new Enums();
    }

    function test_initialStatus() public view {
        assert(enums.currentStatus() == Enums.Status.Pending);
    }

    function test_initialSize() public view {
        assert(enums.currentSize() == Enums.Size.Small);
    }

    function test_setStatus() public {
        enums.setStatus(Enums.Status.Active);
        assert(enums.currentStatus() == Enums.Status.Active);
    }

    function test_getStatus() public {
        enums.setStatus(Enums.Status.Completed);
        assert(enums.getStatus() == Enums.Status.Completed);
    }

    function test_isActive_false() public view {
        assert(enums.isActive() == false);
    }

    function test_isActive_true() public {
        enums.setStatus(Enums.Status.Active);
        assert(enums.isActive() == true);
    }

    function test_setSize() public {
        enums.setSize(Enums.Size.Large);
        assert(enums.currentSize() == Enums.Size.Large);
    }

    function test_compareStatus_equal() public view {
        assert(enums.compareStatus(Enums.Status.Active, Enums.Status.Active) == true);
    }

    function test_compareStatus_notEqual() public view {
        assert(enums.compareStatus(Enums.Status.Pending, Enums.Status.Active) == false);
    }

    function test_statusToUint() public view {
        assert(enums.statusToUint(Enums.Status.Pending) == 0);
        assert(enums.statusToUint(Enums.Status.Active) == 1);
        assert(enums.statusToUint(Enums.Status.Completed) == 2);
        assert(enums.statusToUint(Enums.Status.Cancelled) == 3);
    }

    function test_uintToStatus() public view {
        assert(enums.uintToStatus(0) == Enums.Status.Pending);
        assert(enums.uintToStatus(1) == Enums.Status.Active);
        assert(enums.uintToStatus(2) == Enums.Status.Completed);
        assert(enums.uintToStatus(3) == Enums.Status.Cancelled);
    }

    function test_allStatusValues() public {
        enums.setStatus(Enums.Status.Pending);
        assert(enums.currentStatus() == Enums.Status.Pending);
        
        enums.setStatus(Enums.Status.Active);
        assert(enums.currentStatus() == Enums.Status.Active);
        
        enums.setStatus(Enums.Status.Completed);
        assert(enums.currentStatus() == Enums.Status.Completed);
        
        enums.setStatus(Enums.Status.Cancelled);
        assert(enums.currentStatus() == Enums.Status.Cancelled);
    }

    function test_allSizeValues() public {
        enums.setSize(Enums.Size.Small);
        assert(enums.currentSize() == Enums.Size.Small);
        
        enums.setSize(Enums.Size.Medium);
        assert(enums.currentSize() == Enums.Size.Medium);
        
        enums.setSize(Enums.Size.Large);
        assert(enums.currentSize() == Enums.Size.Large);
    }
}
