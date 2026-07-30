//@ run-call: add 2 => 42
//@ run-call: negate(bool) true => false
//@ run-call: pair 41, true => 42, false
//@ run-call: sum(uint256[]) [1, 2, 3] => 6
//@ run-call: increment => 41
//@ run-call: increment => 41
//@ run-call: testInline()
//@ run-call: fullyInitializedNamedStruct => ([0], 0x00)
//@ run-call: sameTupleTarget() => 1
//@ run-call: effectfulTupleLocations() => 12, 10, 20
//@ run-call: captureReturndataBeforeLvalueEffects() => true, 7
//@ run-call: storageBytesPushLvalues() => 5, 18, 52, 86
//@ run-call: reservedSpillFreshness(bool) true => 83
//@ run-call: reservedSpillFreshness(bool) false => 137
//@ run-call: 0x1003e2d20000000000000000000000000000000000000000000000000000000000000002 => 0x000000000000000000000000000000000000000000000000000000000000002a

contract RunCall {
    struct DynamicHolder {
        uint256[] values;
        bytes data;
    }

    uint256 private base;
    uint256[] private tupleValues;
    uint256 private trace;
    mapping(uint256 => bool) private outcomes;
    bytes private captured;
    bytes private pushed;

    constructor() {
        base = 40;
    }

    function add(uint256 value) external view returns (uint256) {
        return base + value;
    }

    function negate(bool value) external pure returns (bool) {
        return !value;
    }

    function pair(uint256 value, bool flag) external pure returns (uint256, bool) {
        return (value + 1, !flag);
    }

    function sum(uint256[] calldata values) external pure returns (uint256 result) {
        for (uint256 i = 0; i < values.length; i++) {
            result += values[i];
        }
    }

    function increment() external returns (uint256) {
        return ++base;
    }

    function testInline() external view {
        assert(base == 40);
    }

    function fullyInitializedNamedStruct()
        external
        pure
        returns (DynamicHolder memory holder)
    {
        holder.values = new uint256[](1);
        holder.data = new bytes(1);
    }

    function sameTupleTarget() external pure returns (uint256 value) {
        (value, value) = (1, 2);
    }

    function firstTupleIndex() internal returns (uint256) {
        trace = trace * 10 + 1;
        return 0;
    }

    function secondTupleIndex() internal returns (uint256) {
        trace = trace * 10 + 2;
        return 1;
    }

    function effectfulTupleLocations() external returns (uint256, uint256, uint256) {
        tupleValues.push(0);
        tupleValues.push(0);
        (tupleValues[firstTupleIndex()], tupleValues[secondTupleIndex()]) = (10, 20);
        return (trace, tupleValues[0], tupleValues[1]);
    }

    function seven() external pure returns (uint256) {
        return 7;
    }

    function one() external pure returns (uint256) {
        return 1;
    }

    function effectfulReturndataIndex() internal returns (uint256) {
        (bool success,) = address(this).call(abi.encodeCall(this.one, ()));
        require(success);
        return 1;
    }

    function captureReturndataBeforeLvalueEffects() external returns (bool, uint256) {
        (outcomes[effectfulReturndataIndex()], captured) =
            address(this).call(abi.encodeCall(this.seven, ()));
        return (outcomes[1], abi.decode(captured, (uint256)));
    }

    function storageBytesPushLvalues() external returns (uint256, uint8, uint8, uint8) {
        pushed.push(0x01);
        pushed.push(0x02);
        pushed.push() = 0x12;
        (pushed.push(), pushed.push()) = (0x34, 0x56);
        return (pushed.length, uint8(pushed[2]), uint8(pushed[3]), uint8(pushed[4]));
    }

    function reservedSpillFreshness(bool first) external returns (uint256 out) {
        uint256 seed = base;
        uint256 a = seed;
        uint256 off = seed;
        if (first) {
            (a, off) = pairInternal(seed);
            out = a + off;
        } else {
            base = 99;
            (uint256 b, uint256 c) = pairInternal(off + 7);
            out = b + c + off;
        }
    }

    function pairInternal(uint256 value) internal pure returns (uint256, uint256) {
        return (value + 1, value + 2);
    }
}
