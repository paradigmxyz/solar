// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Counter.sol";
import "../src/Mapping.sol";
import "../src/Events.sol";
import "../src/MultipleReturns.sol";
import "../src/Modifiers.sol";
import "../src/Inheritance.sol";

/// @title EquivalenceTest - Validates Solar bytecode produces same behavior as solc
/// @dev This test file is compiled twice:
///      1. With solc (default profile) - generates solc bytecode
///      2. With solar (solar profile) - generates solar bytecode
///      Run both and compare outputs to verify equivalence.
contract EquivalenceTest {
    Counter public counter;

    function setUp() public {
        counter = new Counter();
    }

    function test_initialCountIsZero() public view {
        assert(counter.count() == 0);
        assert(counter.getCount() == 0);
    }

    function test_increment() public {
        counter.increment();
        assert(counter.count() == 1);
    }

    function test_incrementMultiple() public {
        counter.increment();
        counter.increment();
        counter.increment();
        assert(counter.count() == 3);
    }

    function test_getCount() public {
        counter.increment();
        uint256 c = counter.getCount();
        assert(c == 1);
    }

    function test_getCountMatchesPublicVar() public {
        counter.increment();
        counter.increment();
        assert(counter.count() == counter.getCount());
    }
}

/// @title MappingTest - Tests for mapping storage operations
contract MappingTest {
    Mapping public m;

    function setUp() public {
        m = new Mapping();
    }

    function test_initialBalanceIsZero() public view {
        assert(m.get(address(1)) == 0);
        assert(m.balances(address(1)) == 0);
    }

    function test_setAndGet() public {
        m.set(address(1), 100);
        assert(m.get(address(1)) == 100);
    }

    function test_setMultipleAddresses() public {
        m.set(address(1), 100);
        m.set(address(2), 200);
        m.set(address(3), 300);
        assert(m.get(address(1)) == 100);
        assert(m.get(address(2)) == 200);
        assert(m.get(address(3)) == 300);
    }

    function test_overwriteValue() public {
        m.set(address(1), 100);
        m.set(address(1), 999);
        assert(m.get(address(1)) == 999);
    }

    function test_publicMappingGetter() public {
        m.set(address(42), 12345);
        assert(m.balances(address(42)) == 12345);
    }
}

/// @title EventsTest - Tests for event emission
contract EventsTest {
    Events public e;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event ValueSet(uint256 indexed id, uint256 value);

    function setUp() public {
        e = new Events();
    }

    function test_emitTransfer() public {
        e.emitTransfer(address(1), address(2), 100);
    }

    function test_setValue() public {
        e.setValue(42, 999);
        assert(e.lastValue() == 999);
    }

    function test_multipleEvents() public {
        e.emitTransfer(address(1), address(2), 100);
        e.emitTransfer(address(3), address(4), 200);
        e.setValue(1, 111);
        e.setValue(2, 222);
        assert(e.lastValue() == 222);
    }
}

/// @title MultipleReturnsTest - Tests for functions with multiple return values
contract MultipleReturnsTest {
    MultipleReturns public mr;

    function setUp() public {
        mr = new MultipleReturns();
    }

    function test_initialValues() public view {
        (uint256 a, uint256 b) = mr.getTwo();
        assert(a == 0);
        assert(b == 0);
    }

    function test_getTwo() public {
        mr.setValues(10, 20);
        (uint256 a, uint256 b) = mr.getTwo();
        assert(a == 10);
        assert(b == 20);
    }

    function test_getThree() public {
        mr.setValues(5, 7);
        (uint256 a, uint256 b, uint256 c) = mr.getThree();
        assert(a == 5);
        assert(b == 7);
        assert(c == 12);
    }

    function test_getSwapped() public {
        mr.setValues(100, 200);
        (uint256 b, uint256 a) = mr.getSwapped();
        assert(a == 100);
        assert(b == 200);
    }
}

/// @title ModifiersTest - Tests for modifier behavior
contract ModifiersTest {
    Modifiers public mod;

    function setUp() public {
        mod = new Modifiers();
    }

    function test_ownerIsDeployer() public view {
        assert(mod.owner() == address(this));
    }

    function test_setValue() public {
        mod.setValue(42);
        assert(mod.value() == 42);
    }

    function test_setPaused() public {
        mod.setPaused(true);
        assert(mod.paused() == true);
        mod.setPaused(false);
        assert(mod.paused() == false);
    }

    function test_transferOwnership() public {
        mod.transferOwnership(address(1));
        assert(mod.owner() == address(1));
    }

    function test_setValueWhenPausedReverts() public {
        mod.setPaused(true);
        try mod.setValue(99) {
            assert(false);
        } catch {
            assert(true);
        }
    }

    function test_nonOwnerReverts() public {
        mod.transferOwnership(address(1));
        try mod.setValue(99) {
            assert(false);
        } catch {
            assert(true);
        }
    }
}

/// @title InheritanceTest - Tests for contract inheritance
contract InheritanceTest {
    Base public base;
    Derived public derived;

    function setUp() public {
        base = new Base();
        derived = new Derived();
    }

    function test_baseSetAndGet() public {
        base.setBaseValue(10);
        assert(base.getBaseValue() == 10);
        assert(base.baseValue() == 10);
    }

    function test_derivedOverride() public {
        derived.setBaseValue(10);
        assert(derived.baseValue() == 20);
    }

    function test_derivedOwnFunction() public {
        derived.setDerivedValue(5);
        assert(derived.derivedValue() == 5);
    }

    function test_getSum() public {
        derived.setBaseValue(10);
        derived.setDerivedValue(3);
        assert(derived.getSum() == 23);
    }

    function test_inheritedGetter() public {
        derived.setBaseValue(7);
        assert(derived.getBaseValue() == 14);
    }
}
