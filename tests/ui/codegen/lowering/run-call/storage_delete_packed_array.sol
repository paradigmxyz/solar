//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: clear() => 1, 0, 3
//@[gas] run-call: clear() => 1, 0, 3
//@[size] run-call: clear() => 1, 0, 3
//@[none] run-call: clearReference() => 0, 0, 0
//@[gas] run-call: clearReference() => 0, 0, 0
//@[size] run-call: clearReference() => 0, 0, 0
//@[none] run-call: clearFixed() => 0, 0, 0
//@[gas] run-call: clearFixed() => 0, 0, 0
//@[size] run-call: clearFixed() => 0, 0, 0
//@[none] run-call: clearDynamicDirtyWord() => 0
//@[gas] run-call: clearDynamicDirtyWord() => 0
//@[size] run-call: clearDynamicDirtyWord() => 0
//@[none] run-call: clearFixedDirtyWord() => 0
//@[gas] run-call: clearFixedDirtyWord() => 0
//@[size] run-call: clearFixedDirtyWord() => 0
//@[none] run-call: clearOddWidthDirtyWords() => 0, 0, 0, 0, 0, 0
//@[gas] run-call: clearOddWidthDirtyWords() => 0, 0, 0, 0, 0, 0
//@[size] run-call: clearOddWidthDirtyWords() => 0, 0, 0, 0, 0, 0

contract StorageDeletePackedArray {
    uint8[] private values;
    uint8[3] private fixedValues;
    bytes9[] private oddValues;
    bytes9[7] private oddFixedValues;

    function clear() external returns (uint8 first, uint8 deleted, uint8 last) {
        values.push(1);
        values.push(2);
        values.push(3);
        delete values[1];
        return (values[0], values[1], values[2]);
    }

    function clearReference() external returns (uint8 first, uint8 deleted, uint8 last) {
        fixedValues[0] = 1;
        fixedValues[1] = 2;
        fixedValues[2] = 3;
        uint8[3] storage valuesRef = fixedValues;
        delete valuesRef;
        return (fixedValues[0], fixedValues[1], fixedValues[2]);
    }

    function clearFixed() external returns (uint8 first, uint8 deleted, uint8 last) {
        fixedValues[0] = 1;
        fixedValues[1] = 2;
        fixedValues[2] = 3;
        delete fixedValues;
        return (fixedValues[0], fixedValues[1], fixedValues[2]);
    }

    function clearDynamicDirtyWord() external returns (uint256 word) {
        values.push();
        values.push();
        values.push();
        uint256 dataSlot;
        assembly {
            mstore(0, values.slot)
            dataSlot := keccak256(0, 32)
            sstore(dataSlot, not(0))
        }
        delete values;
        assembly {
            word := sload(dataSlot)
        }
    }

    function clearFixedDirtyWord() external returns (uint256 word) {
        assembly {
            sstore(fixedValues.slot, not(0))
        }
        delete fixedValues;
        assembly {
            word := sload(fixedValues.slot)
        }
    }

    function clearOddWidthDirtyWords()
        external
        returns (uint256, uint256, uint256, uint256, uint256, uint256)
    {
        uint256 dynamicDataSlot;
        assembly {
            sstore(oddValues.slot, 7)
            mstore(0, oddValues.slot)
            dynamicDataSlot := keccak256(0, 32)
            sstore(dynamicDataSlot, not(0))
            sstore(add(dynamicDataSlot, 1), not(0))
            sstore(add(dynamicDataSlot, 2), not(0))
            sstore(oddFixedValues.slot, not(0))
            sstore(add(oddFixedValues.slot, 1), not(0))
            sstore(add(oddFixedValues.slot, 2), not(0))
        }
        delete oddValues;
        delete oddFixedValues;
        assembly {
            mstore(0, sload(dynamicDataSlot))
            mstore(32, sload(add(dynamicDataSlot, 1)))
            mstore(64, sload(add(dynamicDataSlot, 2)))
            mstore(96, sload(oddFixedValues.slot))
            mstore(128, sload(add(oddFixedValues.slot, 1)))
            mstore(160, sload(add(oddFixedValues.slot, 2)))
            return(0, 192)
        }
    }
}
