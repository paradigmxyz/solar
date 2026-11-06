//@ compile-flags: -O gas
//@ run-call: f => 1

// A pointer that conditional steps back have merged sixty times reaches each earlier merge by
// two paths. The check whether a write through it may land below the heap looks at each word
// once, not once per path, and gives up with the answer that it may.

contract C {
    function f() external pure returns (uint256) {
        assembly {
            let p := mload(0x40)
            if and(calldataload(4), 1) { p := sub(p, 1) }
            if and(calldataload(4), 2) { p := sub(p, 1) }
            if and(calldataload(4), 4) { p := sub(p, 1) }
            if and(calldataload(4), 8) { p := sub(p, 1) }
            if and(calldataload(4), 16) { p := sub(p, 1) }
            if and(calldataload(4), 32) { p := sub(p, 1) }
            if and(calldataload(4), 64) { p := sub(p, 1) }
            if and(calldataload(4), 128) { p := sub(p, 1) }
            if and(calldataload(4), 256) { p := sub(p, 1) }
            if and(calldataload(4), 512) { p := sub(p, 1) }
            if and(calldataload(4), 1024) { p := sub(p, 1) }
            if and(calldataload(4), 2048) { p := sub(p, 1) }
            if and(calldataload(4), 4096) { p := sub(p, 1) }
            if and(calldataload(4), 8192) { p := sub(p, 1) }
            if and(calldataload(4), 16384) { p := sub(p, 1) }
            if and(calldataload(4), 32768) { p := sub(p, 1) }
            if and(calldataload(4), 65536) { p := sub(p, 1) }
            if and(calldataload(4), 131072) { p := sub(p, 1) }
            if and(calldataload(4), 262144) { p := sub(p, 1) }
            if and(calldataload(4), 524288) { p := sub(p, 1) }
            if and(calldataload(4), 1048576) { p := sub(p, 1) }
            if and(calldataload(4), 2097152) { p := sub(p, 1) }
            if and(calldataload(4), 4194304) { p := sub(p, 1) }
            if and(calldataload(4), 8388608) { p := sub(p, 1) }
            if and(calldataload(4), 16777216) { p := sub(p, 1) }
            if and(calldataload(4), 33554432) { p := sub(p, 1) }
            if and(calldataload(4), 67108864) { p := sub(p, 1) }
            if and(calldataload(4), 134217728) { p := sub(p, 1) }
            if and(calldataload(4), 268435456) { p := sub(p, 1) }
            if and(calldataload(4), 536870912) { p := sub(p, 1) }
            if and(calldataload(4), 1073741824) { p := sub(p, 1) }
            if and(calldataload(4), 2147483648) { p := sub(p, 1) }
            if and(calldataload(4), 4294967296) { p := sub(p, 1) }
            if and(calldataload(4), 8589934592) { p := sub(p, 1) }
            if and(calldataload(4), 17179869184) { p := sub(p, 1) }
            if and(calldataload(4), 34359738368) { p := sub(p, 1) }
            if and(calldataload(4), 68719476736) { p := sub(p, 1) }
            if and(calldataload(4), 137438953472) { p := sub(p, 1) }
            if and(calldataload(4), 274877906944) { p := sub(p, 1) }
            if and(calldataload(4), 549755813888) { p := sub(p, 1) }
            if and(calldataload(4), 1099511627776) { p := sub(p, 1) }
            if and(calldataload(4), 2199023255552) { p := sub(p, 1) }
            if and(calldataload(4), 4398046511104) { p := sub(p, 1) }
            if and(calldataload(4), 8796093022208) { p := sub(p, 1) }
            if and(calldataload(4), 17592186044416) { p := sub(p, 1) }
            if and(calldataload(4), 35184372088832) { p := sub(p, 1) }
            if and(calldataload(4), 70368744177664) { p := sub(p, 1) }
            if and(calldataload(4), 140737488355328) { p := sub(p, 1) }
            if and(calldataload(4), 281474976710656) { p := sub(p, 1) }
            if and(calldataload(4), 562949953421312) { p := sub(p, 1) }
            if and(calldataload(4), 1125899906842624) { p := sub(p, 1) }
            if and(calldataload(4), 2251799813685248) { p := sub(p, 1) }
            if and(calldataload(4), 4503599627370496) { p := sub(p, 1) }
            if and(calldataload(4), 9007199254740992) { p := sub(p, 1) }
            if and(calldataload(4), 18014398509481984) { p := sub(p, 1) }
            if and(calldataload(4), 36028797018963968) { p := sub(p, 1) }
            if and(calldataload(4), 72057594037927936) { p := sub(p, 1) }
            if and(calldataload(4), 144115188075855872) { p := sub(p, 1) }
            if and(calldataload(4), 288230376151711744) { p := sub(p, 1) }
            if and(calldataload(4), 576460752303423488) { p := sub(p, 1) }
            mstore(p, 1)
            mstore(0x40, add(mload(0x40), 0x20))
        }
        return 1;
    }
}
