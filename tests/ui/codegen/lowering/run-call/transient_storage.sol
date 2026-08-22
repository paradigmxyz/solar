//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: TransientStorage::separateSpaces(uint256,uint256) 11, 22 => 11, 22, 11, 22, 0, 0
//@ run-call: TransientStorage::packed() => 4386, 13124, 860098850
//@ run-call: TransientStorage::signed() => -2
//@ run-call: TransientStorage::clear() => 0, 0
//@ run-call: TransientStorage::getter() => 77
//@ run-call: TransientModifier::run() => 100
// ported-from: test/libsolidity/semanticTests/operators/compound_assign_transient_storage.sol
// ported-from: test/libsolidity/semanticTests/modifiers/transient_state_variable_value_type.sol

contract TransientStorage {
    uint256 persistent;
    uint256 public transient temporary;
    uint16 transient low;
    uint16 transient high;
    int16 transient signedValue;

    function separateSpaces(uint256 stored, uint256 temporaryValue)
        external
        returns (
            uint256 storedValue,
            uint256 transientValue,
            uint256 storedWord,
            uint256 transientWord,
            uint256 storedSlot,
            uint256 transientSlot
        )
    {
        persistent = stored;
        temporary = temporaryValue;
        storedValue = persistent;
        transientValue = temporary;
        assembly {
            storedWord := sload(persistent.slot)
            transientWord := tload(temporary.slot)
            storedSlot := persistent.slot
            transientSlot := temporary.slot
        }
    }

    function packed() external returns (uint16, uint16, uint256 word) {
        low = 0x1122;
        high = 0x3344;
        assembly {
            word := tload(low.slot)
        }
        return (low, high, word);
    }

    function signed() external returns (int16) {
        signedValue = -2;
        return signedValue;
    }

    function clear() external returns (uint256 value, uint256 word) {
        temporary = 42;
        delete temporary;
        value = temporary;
        assembly {
            word := tload(temporary.slot)
        }
    }

    function getter() external returns (uint256) {
        temporary = 77;
        return this.temporary();
    }
}

contract TransientModifier {
    uint16 transient value;

    modifier bump(uint16) {
        value += 10;
        _;
    }

    function run() external bump(value) returns (uint16) {
        value *= 10;
        return value;
    }
}
