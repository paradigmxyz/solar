//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=mir
// Fixed storage arrays of multi-slot elements stride by the element's slot
// count: arr[i].a lives at base + i*2 and arr[i].b one slot above, so
// adjacent elements do not overlap.
contract StorageStructArray {
    struct S {
        uint256 a;
        uint256 b;
    }

    S[3] arr;

    function setS(uint256 i, uint256 x, uint256 y) public {
        arr[i].a = x;
        arr[i].b = y;
    }

    function getS(uint256 i) public view returns (uint256, uint256) {
        return (arr[i].a, arr[i].b);
    }
}
