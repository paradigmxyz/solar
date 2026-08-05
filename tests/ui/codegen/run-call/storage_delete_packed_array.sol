//@ run-call: clear() => 1, 0, 3
//@ run-call: clearReference() => 0, 0, 0
//@ run-call: clearFixed() => 0, 0, 0

contract StorageDeletePackedArray {
    uint8[] private values;
    uint8[3] private fixedValues;

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
}
