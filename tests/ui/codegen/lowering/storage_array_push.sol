//@ run-call: direct 7 => 7, 1
//@ run-call: pushResult => 0, 1
//@ run-call: pushLvalue 11 => 11, 1
//@ run-call: throughReference 13 => 13, 1
//@ run-call: nestedLvalue 17 => 17, 1, 1
//@ run-call: mappingValue 19 => 19, 1
//@ run-call: structField 23 => 23, 1
//@ run-call: structLvalue 29 => 29, 1
//@ run-call: bytesValue => 1
//@ run-call: structValue 41, 43 => 41, 43, 1
//@ run-call: structPopThenPush => 0, 1
//@ run-call: popThenPush => 0, 1
//@ run-call-fail: popEmpty() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000031

contract StorageArrayPush {
    struct Bucket {
        uint256[] values;
    }

    struct Pair {
        uint256 first;
        uint256 second;
    }

    uint256[] values;
    uint256[][] nested;
    mapping(uint256 => uint256[]) mapped;
    Bucket bucket;
    Pair[] pairs;
    bytes[] byteValues;

    function direct(uint256 value) external returns (uint256, uint256) {
        values.push(value);
        return (values[0], values.length);
    }

    function pushResult() external returns (uint256, uint256) {
        uint256 pushed = values.push();
        return (pushed, values.length);
    }

    function pushLvalue(uint256 value) external returns (uint256, uint256) {
        values.push() = value;
        return (values[0], values.length);
    }

    function throughReference(uint256 value) external returns (uint256, uint256) {
        uint256[] storage arrayRef = values;
        arrayRef.push(value);
        return (values[0], values.length);
    }

    function nestedLvalue(uint256 value) external returns (uint256, uint256, uint256) {
        nested.push().push() = value;
        return (nested[0][0], nested.length, nested[0].length);
    }

    function mappingValue(uint256 value) external returns (uint256, uint256) {
        mapped[1].push(value);
        return (mapped[1][0], mapped[1].length);
    }

    function structField(uint256 value) external returns (uint256, uint256) {
        bucket.values.push(value);
        return (bucket.values[0], bucket.values.length);
    }

    function structLvalue(uint256 value) external returns (uint256, uint256) {
        pairs.push().first = value;
        return (pairs[0].first, pairs.length);
    }

    function bytesValue() external returns (uint256) {
        bytes memory value = hex"010203";
        byteValues.push(value);
        return byteValues.length;
    }

    function structValue(uint256 first, uint256 second)
        external
        returns (uint256, uint256, uint256)
    {
        pairs.push(Pair(first, second));
        return (pairs[0].first, pairs[0].second, pairs.length);
    }

    function structPopThenPush() external returns (uint256, uint256) {
        pairs.push().first = 31;
        pairs.pop();
        return (pairs.push().first, pairs.length);
    }

    function popThenPush() external returns (uint256, uint256) {
        values.push(37);
        values.pop();
        uint256 pushed = values.push();
        return (pushed, values.length);
    }

    function popEmpty() external {
        values.pop();
    }
}
