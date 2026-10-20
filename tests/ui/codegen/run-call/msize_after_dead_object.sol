//@ codegen-matrix: standard
//@ run-call: decoded() => true

contract Test {
    struct S {
        uint256 a;
        uint256 b;
    }

    // Reading one field of a decoded struct leaves the object's stores dead. `msize()` must still
    // cover the free memory pointer.
    function decoded() external pure returns (bool ok) {
        S memory s = abi.decode(abi.encode(S(1, 2)), (S));
        uint256 a = s.a;
        uint256 size;
        uint256 fmp;
        assembly {
            size := msize() //~[gas,size] WARN: `msize` is not exact when optimizing
            fmp := mload(0x40)
        }
        ok = a == 1 && size >= fmp;
    }
}
