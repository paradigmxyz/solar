//@ codegen-matrix: standard
//@ run-call: MemoryStructAssemblyList::defaultLocal => true
//@ run-call: MemoryStructAssemblyList::defaultReturn => true
//@ run-call: MemoryStructAssemblyList::firstAppend => true
//@ run-call: MemoryStructAssemblyList::secondAppend => true
//@ run-call: MemoryStructAssemblyList::recursiveYul 0 => 1, 0xc0
//@ run-call: MemoryStructAssemblyList::recursiveYul 3 => 4, 0xc3
//@ run-call: MemoryStructAssemblyList::nestedListEncoding => 8, 0xc7c0c1c0c3c0c1c0000000000000000000000000000000000000000000000000

contract MemoryStructAssemblyList {
    struct List {
        uint256 data;
    }

    function empty() internal pure returns (List memory result) {}

    function append(List memory list, uint256 x) internal pure returns (List memory result) {
        result.data = x << 48;
        updateTail(list, result);
        result = list;
    }

    function append(List memory list, List memory x) internal pure returns (List memory result) {
        assembly {
            mstore(result, shl(40, or(3, shl(8, x))))
        }
        updateTail(list, result);
        result = list;
    }

    function updateTail(List memory list, List memory result) private pure {
        assembly {
            let v := or(shr(mload(list), result), mload(list))
            let tail := shr(40, v)
            mstore(list, xor(shl(40, xor(tail, result)), v))
            mstore(tail, or(mload(tail), result))
        }
    }

    function dirtyFreeMemory() internal view {
        assembly {
            let offset := mload(0x40)
            mstore(offset, add(caller(), gas()))
            mstore(add(offset, 0x20), not(0))
        }
    }

    function defaultLocal() external view returns (bool ok) {
        List memory list;
        dirtyFreeMemory();
        assembly {
            ok := iszero(mload(list))
        }
    }

    function defaultReturn() external view returns (bool ok) {
        dirtyFreeMemory();
        List memory list = empty();
        assembly {
            ok := iszero(mload(list))
        }
    }

    function firstAppend() external view returns (bool ok) {
        List memory list;
        dirtyFreeMemory();
        list = append(list, 7);
        assembly {
            let packed := mload(list)
            let head := and(packed, 0xffffffffff)
            let tail := shr(40, packed)
            ok := and(and(eq(head, tail), iszero(iszero(head))), eq(shr(48, mload(head)), 7))
        }
    }

    function secondAppend() external view returns (bool ok) {
        List memory list;
        dirtyFreeMemory();
        list = append(list, 7);
        list = append(list, 9);
        assembly {
            let packed := mload(list)
            let head := and(packed, 0xffffffffff)
            let tail := shr(40, packed)
            ok := and(eq(and(mload(head), 0xffffffffff), tail), eq(shr(48, mload(tail)), 9))
        }
    }

    function recursiveYul(uint256 depth) external pure returns (uint256 length, bytes1 first) {
        assembly {
            function encode(n, out) -> end {
                if iszero(n) {
                    mstore8(out, 0xc0)
                    end := add(out, 1)
                    leave
                }
                end := encode(sub(n, 1), add(out, 1))
                mstore8(out, add(0xc0, n))
            }
            let out := mload(0x40)
            let end := encode(depth, out)
            length := sub(end, out)
            first := mload(out)
        }
    }


    function nestedListEncoding() external pure returns (uint256 length, bytes32 word) {
        List memory list;
        list = append(list, empty());
        list = append(list, append(empty(), empty()));
        list = append(list, append(append(empty(), empty()), append(empty(), empty())));
        assembly {
            function encodeUint(x, out) -> end {
                end := add(out, 1)
                if iszero(gt(x, 0x7f)) {
                    mstore8(out, or(shl(7, iszero(x)), x))
                    leave
                }
                let r := shl(7, lt(0xffffffffffffffffffffffffffffffff, x))
                r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
                r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
                r := or(r, shl(4, lt(0xffff, shr(r, x))))
                r := or(shr(3, r), lt(0xff, shr(r, x)))
                mstore8(out, add(r, 0x81))
                mstore(0x00, x)
                mstore(end, mload(xor(31, r)))
                end := add(add(1, r), end)
            }
            function encodeAddress(x, out) -> end {
                end := add(out, 0x15)
                mstore(out, shl(88, x))
                mstore8(out, 0x94)
            }
            function encodeBytes(x, out, prefix) -> end {
                end := add(out, 1)
                let n := mload(x)
                if iszero(gt(n, 55)) {
                    let first := mload(add(0x20, x))
                    if iszero(and(eq(1, n), lt(byte(0, first), 0x80))) {
                        mstore8(out, add(n, prefix))
                        mstore(add(0x21, out), mload(add(0x40, x)))
                        mstore(end, first)
                        end := add(n, end)
                        leave
                    }
                    mstore(out, first)
                    leave
                }
                returndatacopy(returndatasize(), returndatasize(), shr(32, n))
                let r := add(1, add(lt(0xff, n), add(lt(0xffff, n), lt(0xffffff, n))))
                mstore(out, shl(248, add(r, add(prefix, 55))))
                let cursor := add(r, end)
                end := add(cursor, n)
                for { let delta := sub(add(0x20, x), cursor) } 1 {} {
                    mstore(cursor, mload(add(delta, cursor)))
                    cursor := add(cursor, 0x20)
                    if iszero(lt(cursor, end)) { break }
                }
                mstore(out, or(mload(out), shl(sub(248, shl(3, r)), n)))
            }
            function encodeList(l, out) -> end {
                if iszero(mload(l)) {
                    mstore8(out, 0xc0)
                    end := add(out, 1)
                    leave
                }
                let cursor := add(out, 0x20)
                for { let head := l } 1 {} {
                    head := and(mload(head), 0xffffffffff)
                    if iszero(head) { break }
                    let kind := byte(26, mload(head))
                    if iszero(gt(kind, 1)) {
                        if iszero(kind) {
                            cursor := encodeUint(shr(48, mload(head)), cursor)
                            continue
                        }
                        cursor := encodeUint(mload(shr(48, mload(head))), cursor)
                        continue
                    }
                    if eq(kind, 2) {
                        cursor := encodeBytes(shr(48, mload(head)), cursor, 0x80)
                        continue
                    }
                    if eq(kind, 3) {
                        cursor := encodeList(shr(48, mload(head)), cursor)
                        continue
                    }
                    cursor := encodeAddress(shr(48, mload(head)), cursor)
                }
                let size := sub(cursor, add(out, 0x20))
                if iszero(gt(size, 55)) {
                    mstore8(out, add(size, 0xc0))
                    mstore(add(out, 1), mload(add(out, 0x20)))
                    mstore(add(out, 0x21), mload(add(out, 0x40)))
                    end := add(size, add(out, 1))
                    leave
                }
                mstore(out, size)
                end := encodeBytes(out, out, 0xc0)
            }
            let out := mload(0x40)
            let end := encodeList(list, out)
            length := sub(end, out)
            word := mload(out)
        }
    }
}
