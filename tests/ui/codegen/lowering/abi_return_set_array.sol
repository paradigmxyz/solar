//@ revisions: size gas none
//@[size] compile-flags: -Osize -Zdump=mir
//@[size] filecheck:
//@[gas] compile-flags: -Ogas
//@[none] compile-flags: -O none
//@ run-call: merged [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000003], [0x0000000000000000000000000000000000000002] => [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000003]
//@ run-call: merged [], [] => []
//@ run-call: dirtied 1 => [0x0000000000000000000000000000000000000001]

import {WordArrays} from "solar:core/v1/WordArrays.sol";

// Size builds share one set helper between address and word arrays. Its
// comparison flip is a scalar and adds no words to the array it returns, whose
// words all come from its array parameters: validated addresses here. The
// wrapper returns what an internal call passed straight through from that
// helper, so the result needs no per-element cleanup.
// CHECK-LABEL: fn @merged{{[( ]}}
// CHECK-NOT: 0xffffffffffffffffffffffffffffffffffffffff
// CHECK: returndata
contract SetArray {
    function union(address[] memory a, address[] memory b)
        internal
        pure
        returns (address[] memory)
    {
        return WordArrays.union(a, b);
    }

    function merged(address[] memory a, address[] memory b)
        external
        pure
        returns (address[] memory)
    {
        return union(a, b);
    }
}

// A word that inline assembly wrote proves nothing, so the returned array is
// still cleaned.
// CHECK-LABEL: fn @dirtied{{[( ]}}
// CHECK: and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
contract DirtySetArray {
    function union(address[] memory a, address[] memory b)
        internal
        pure
        returns (address[] memory)
    {
        return WordArrays.union(a, b);
    }

    function dirtied(uint256 x) external pure returns (address[] memory) {
        address[] memory a = new address[](1);
        assembly {
            mstore(add(a, 0x20), or(shl(160, 0xff), x))
        }
        return union(a, new address[](0));
    }
}
