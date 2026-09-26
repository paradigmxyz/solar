//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
import {Strings} from "solar:core/v1/Strings.sol";

contract Test {
    // Below sixteen bytes the prefix and the digits fit one data word: the
    // width test reverts before anything is written, and a fixed allocation
    // takes the length and a single text store. A width below three bytes
    // spreads its digits in one four-lane group, with a store of its own.
    // CHECK-LABEL: fn @spell
    // CHECK: = lt arg1, 16
    // CHECK: = lt arg1, 3
    // CHECK: = and {{v[0-9]+}}, 0xff00ff{{$}}
    // CHECK: [[TEXT:v[0-9]+]] = or {{v[0-9]+}}, 0x3078000000000000000000000000000000000000000000000000000000000000
    // CHECK-NEXT: [[FMP:v[0-9]+]] = mload 64
    // CHECK-NOT: jumpi
    // CHECK: mstore 64,
    // CHECK: mstore [[FMP]],
    // CHECK: mstore {{v[0-9]+}}, [[TEXT]]
    function spell(uint256 value, uint256 width) external pure returns (string memory) {
        return Strings.toHexString(value, width);
    }
}
