//@ codegen-matrix: standard
//@ run-call: word => 0, 43, 1, 2, 3
//@ run-call: voidCall => 0, 1, 2, 3
//@ run-call: repeated => 0, 0, 44, 45
//@ run-call: twoWords => 7, 8, 1, 2, 3
//@ run-call: staticArray => 1, 2, 1, 2, 3
//@ run-call: dynamicReturn => 3, 1, 2, 3
//@ run-call: dynamicInput => 4, 1, 2, 3
//@ run-call: memoryArgument => 0, 5, 1, 2, 3
//@ run-call: noArguments => 0, 1, 1, 2, 3
//@ run-call: addressArgument => 0, 1, 1, 2, 3
//@ run-call: boolArgument => 0, 1, 1, 2, 3
//@ run-call: narrowReturn => 0, 1
//@ run-call: functionArgument => 0, 7, 1, 2, 3
//@ run-call: internalTuple => 0, 52, 1, 2, 3
//@ run-call: pointerCall => 0, 43, 1, 2, 3

// A call whose input nothing keeps stages it at the free memory pointer without reserving it, so
// the free memory pointer does not move across the call. Memory below the pointer stays intact,
// and a return that overlays the input (more than one word, or a static aggregate) still decodes.
interface ITarget {
    function balanceOf(uint256 account) external view returns (uint256);
    function ping(uint256 value) external;
    function pair(uint256 value) external view returns (uint256, uint256);
    function fixedArray(uint256 value) external view returns (uint256[2] memory);
    function bytesOf(uint256 length) external view returns (bytes memory);
    function sum(uint256[] memory values) external view returns (uint256);
    function sumFixed(uint256[2] memory values) external view returns (uint256);
    function one() external view returns (uint256);
    function owned(address account) external view returns (bool);
    function flag(bool value) external view returns (uint256);
    function small(uint256 value) external view returns (uint8);
    function selectorOf(function() external view returns (uint256) callback) external view returns (uint256);
}

contract Target is ITarget {
    uint256 public pings;

    function balanceOf(uint256 account) external pure returns (uint256) {
        return account + 42;
    }

    function ping(uint256) external {
        pings++;
    }

    function pair(uint256 value) external pure returns (uint256, uint256) {
        return (value, value + 1);
    }

    function fixedArray(uint256 value) external pure returns (uint256[2] memory values) {
        values[0] = value;
        values[1] = value + 1;
    }

    function bytesOf(uint256 length) external pure returns (bytes memory data) {
        data = new bytes(length);
        for (uint256 i; i < length; ++i) {
            data[i] = bytes1(uint8(i + 1));
        }
    }

    function sum(uint256[] memory values) external pure returns (uint256 total) {
        for (uint256 i; i < values.length; ++i) {
            total += values[i];
        }
    }

    function sumFixed(uint256[2] memory values) external pure returns (uint256) {
        return values[0] + values[1];
    }

    function one() external pure returns (uint256) {
        return 1;
    }

    function owned(address account) external pure returns (bool) {
        return account == address(0x1234);
    }

    function flag(bool value) external pure returns (uint256) {
        return value ? 1 : 0;
    }

    function small(uint256 value) external pure returns (uint8) {
        return uint8(value);
    }

    function selectorOf(function() external view returns (uint256) callback)
        external
        view
        returns (uint256)
    {
        return callback();
    }
}

contract ExternalCallInputScratch {
    ITarget internal target = new Target();

    function live() internal pure returns (uint256[] memory values) {
        values = new uint256[](3);
        values[0] = 1;
        values[1] = 2;
        values[2] = 3;
    }

    function fmp() internal pure returns (uint256 ptr) {
        assembly {
            ptr := mload(0x40)
        }
    }

    function word() external view returns (uint256 moved, uint256 value, uint256, uint256, uint256) {
        uint256[] memory values = live();
        uint256 before = fmp();
        value = target.balanceOf(1);
        moved = fmp() - before;
        return (moved, value, values[0], values[1], values[2]);
    }

    function voidCall() external returns (uint256 moved, uint256, uint256, uint256) {
        uint256[] memory values = live();
        uint256 before = fmp();
        target.ping(5);
        moved = fmp() - before;
        return (moved, values[0], values[1], values[2]);
    }

    function repeated() external view returns (uint256 first, uint256 second, uint256 a, uint256 b) {
        uint256 before = fmp();
        a = target.balanceOf(2);
        first = fmp() - before;
        b = target.balanceOf(3);
        second = fmp() - before;
    }

    function twoWords() external view returns (uint256 a, uint256 b, uint256, uint256, uint256) {
        uint256[] memory values = live();
        (a, b) = target.pair(7);
        return (a, b, values[0], values[1], values[2]);
    }

    function staticArray() external view returns (uint256 a, uint256 b, uint256, uint256, uint256) {
        uint256[] memory values = live();
        uint256[2] memory result = target.fixedArray(1);
        return (result[0], result[1], values[0], values[1], values[2]);
    }

    function dynamicReturn() external view returns (uint256 length, uint256, uint256, uint256) {
        uint256[] memory values = live();
        bytes memory data = target.bytesOf(3);
        return (uint256(uint8(data[2])), values[0], values[1], values[2]);
    }

    function dynamicInput() external view returns (uint256 total, uint256, uint256, uint256) {
        uint256[] memory values = live();
        uint256[] memory extra = new uint256[](2);
        extra[0] = 1;
        extra[1] = 3;
        total = target.sum(extra);
        return (total, values[0], values[1], values[2]);
    }

    function memoryArgument() external view returns (uint256 moved, uint256 total, uint256, uint256, uint256) {
        uint256[] memory values = live();
        uint256[2] memory fixedValues = [uint256(2), 3];
        uint256 before = fmp();
        total = target.sumFixed(fixedValues);
        moved = fmp() - before;
        return (moved, total, values[0], values[1], values[2]);
    }

    function noArguments() external view returns (uint256 moved, uint256 value, uint256, uint256, uint256) {
        uint256[] memory values = live();
        uint256 before = fmp();
        value = target.one();
        moved = fmp() - before;
        return (moved, value, values[0], values[1], values[2]);
    }

    function addressArgument() external view returns (uint256 moved, uint256 value, uint256, uint256, uint256) {
        uint256[] memory values = live();
        uint256 before = fmp();
        bool owner = target.owned(address(0x1234));
        moved = fmp() - before;
        return (moved, owner ? 1 : 0, values[0], values[1], values[2]);
    }

    function boolArgument() external view returns (uint256 moved, uint256 value, uint256, uint256, uint256) {
        uint256[] memory values = live();
        uint256 before = fmp();
        value = target.flag(true);
        moved = fmp() - before;
        return (moved, value, values[0], values[1], values[2]);
    }

    function narrowReturn() external view returns (uint256 moved, uint256 value) {
        uint256 before = fmp();
        value = target.small(0x101);
        moved = fmp() - before;
    }

    function functionArgument() external view returns (uint256 moved, uint256 value, uint256, uint256, uint256) {
        uint256[] memory values = live();
        uint256 before = fmp();
        value = target.selectorOf(this.seven);
        moved = fmp() - before;
        return (moved, value, values[0], values[1], values[2]);
    }

    function seven() external pure returns (uint256) {
        return 7;
    }

    function pairValues() internal pure returns (uint256, uint256) {
        return (10, 10);
    }

    function internalTuple() external view returns (uint256 moved, uint256 value, uint256, uint256, uint256) {
        uint256[] memory values = live();
        uint256 before = fmp();
        (, uint256 second) = pairValues();
        value = target.balanceOf(second);
        moved = fmp() - before;
        return (moved, value, values[0], values[1], values[2]);
    }

    function pointerCall() external view returns (uint256 moved, uint256 value, uint256, uint256, uint256) {
        uint256[] memory values = live();
        function(uint256) external view returns (uint256) callback = target.balanceOf;
        uint256 before = fmp();
        value = callback(1);
        moved = fmp() - before;
        return (moved, value, values[0], values[1], values[2]);
    }
}
