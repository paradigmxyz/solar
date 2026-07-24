//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:
// Fixed storage arrays of multi-slot elements stride by the element's slot
// count: arr[i].a lives at base + i*2 and arr[i].b one slot above, so
// adjacent elements do not overlap.
contract StorageStructArray {
    struct S {
        uint256 a;
        uint256 b;
    }

    S[3] arr;

    // CHECK-LABEL: fn @setS{{[( ]}}
    // CHECK: [[OFFSET:v[0-9]+]] = mul arg0, 2
    // CHECK: sstore {{v[0-9]+}}, arg1 !metadata(storage=symbolic([[OFFSET]]))
    // CHECK: [[OFFSET2:v[0-9]+]] = mul arg0, 2
    // CHECK: sstore {{v[0-9]+}}, arg2 !metadata(storage=offset([[OFFSET2]], 1))
    function setS(uint256 i, uint256 x, uint256 y) public {
        arr[i].a = x;
        arr[i].b = y;
    }

    // CHECK-LABEL: fn @getS{{[( ]}}
    // CHECK: [[OFFSET:v[0-9]+]] = mul arg0, 2
    // CHECK: sload {{v[0-9]+}} !metadata(storage=symbolic([[OFFSET]]))
    // CHECK: [[OFFSET2:v[0-9]+]] = mul arg0, 2
    // CHECK: sload {{v[0-9]+}} !metadata(storage=offset([[OFFSET2]], 1))
    // CHECK: returndata 128, 64
    function getS(uint256 i) public view returns (uint256, uint256) {
        return (arr[i].a, arr[i].b);
    }
}
