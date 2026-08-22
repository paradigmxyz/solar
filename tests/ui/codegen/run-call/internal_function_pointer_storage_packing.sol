//@ run-call: high() => 0x00000000000000000000000000000000000000000000001122334455667788
//@ run-call: arrayMatches() => true

contract InternalFunctionPointerStoragePacking {
    function() internal fp;
    uint64 x;
    function() internal[4] fs;

    constructor() {
        fp = a;
        x = 0x1122334455667788;
        fs[0] = a;
        fs[1] = b;
        fs[2] = c;
        fs[3] = d;
    }

    function a() internal {}
    function b() internal {}
    function c() internal {}
    function d() internal {}

    function high() external view returns (uint64 value) {
        assembly {
            value := shr(64, sload(0))
        }
    }

    function arrayMatches() external view returns (bool) {
        assembly {
            let scalar := and(sload(0), 0xffffffffffffffff)
            let first := and(sload(1), 0xffffffffffffffff)
            mstore(0, eq(scalar, first))
            return(0, 32)
        }
    }
}
