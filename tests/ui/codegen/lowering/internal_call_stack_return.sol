//@ revisions: gas size runtime
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[gas] filecheck: --check-prefix=GAS
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@[size] filecheck: --check-prefix=SIZE
//@[runtime] compile-flags: -O gas
//@[runtime] run-call: stackAcross 2 => 215
//@[runtime] run-call: memoryAcross 2 => 215
//@[runtime] run-call: voidAcross 2 => 8
//@[runtime] run-call: nestedAcross 2 => 218
//@[runtime] run-call: multiAcross 2 => 0

contract InternalCallStackReturn {
    uint256 private state;

    // Capture the dispatch targets so checks remain independent of block numbering and layout.
    // GAS-LABEL: @module runtime
    // GAS: push 0x1e388922
    // GAS-NEXT: eq
    // GAS-NEXT: push [[MEMORY_ENTRY:bb[0-9]+]]
    // GAS: push 0x2137370e
    // GAS-NEXT: eq
    // GAS-NEXT: push [[STACK_ENTRY:bb[0-9]+]]
    // GAS: push 0xb1e54a6c
    // GAS-NEXT: eq
    // GAS-NEXT: push [[NESTED_ENTRY:bb[0-9]+]]
    // GAS: push 0xc877cdbb
    // GAS-NEXT: eq
    // GAS-NEXT: push [[MULTI_ENTRY:bb[0-9]+]]
    // GAS: push 0xf368aee0
    // GAS-NEXT: eq
    // GAS-NEXT: push [[VOID_ENTRY:bb[0-9]+]]
    //
    // The stack-return entry carries its multiplication result through the shared argument-store
    // block: only the call argument is stored, then SWAP1 returns with the caller word intact.
    // GAS: [[STACK_ENTRY]]:
    // GAS: push [[STACK_SETUP:bb[0-9]+]]
    // GAS-NEXT: jump [[COMMON_ARGS:bb[0-9]+]]
    // GAS: [[COMMON_ARGS]]:
    // GAS: mul
    // GAS: mstore
    // GAS-NEXT: swap 1
    // GAS-NEXT: jump
    //
    // The memory-return and nested-call entries use the same preserved caller layout.
    // GAS: [[MEMORY_ENTRY]]:
    // GAS: jump [[COMMON_ARGS]]
    // GAS: [[VOID_ENTRY]]:
    // GAS: mul
    // GAS-NEXT: push [[VOID_RETURN:bb[0-9]+]]
    // GAS-NEXT: push 0
    // GAS-NEXT: sload
    // GAS: [[VOID_RETURN]]:
    // GAS-NEXT: push 4
    // GAS-NEXT: calldataload
    // GAS-NEXT: jump [[ADD_RETURN:bb[0-9]+]]
    // GAS: [[NESTED_ENTRY]]:
    // GAS: jump [[COMMON_ARGS]]
    // GAS: [[MULTI_ENTRY]]:
    // GAS: or
    // GAS-NEXT: push [[MULTI_SETUP:bb[0-9]+]]
    // GAS-NEXT: jump [[COMMON_ARGS]]
    // GAS: swap 1
    // GAS-NEXT: addmod

    // Gas mode keeps a one-word helper result on the physical stack and removes its frame slot.
    // Size mode retains the shared memory convention. Five operations keep each leaf above the
    // tiny-leaf inlining threshold so these checks exercise the internal-call conventions.
    //
    // SIZE-LABEL: @module runtime
    // SIZE: push [[RETURN_SLOT:[0-9]+]]
    // SIZE-NEXT: mstore
    // SIZE-NEXT: jump
    // SIZE: push [[RETURN_SLOT]]
    // SIZE-NEXT: mload
    function stackAcross(uint256 x) external pure returns (uint256) {
        unchecked {
            uint256 keep = x * 3;
            return keep + stackHelper(x);
        }
    }

    function stackHelper(uint256 x) internal pure returns (uint256 y) {
        unchecked {
            y = x + 1;
            y *= 3;
            y ^= 5;
            y += 7;
            y *= 11;
        }
    }

    // A memory-returning helper restores the preserved caller word before loading its first
    // result. The following add consumes the load and that caller word directly.
    function memoryAcross(uint256 x) external pure returns (uint256) {
        unchecked {
            uint256 keep = x * 3;
            (uint256 first,) = memoryHelper(x);
            return keep + first;
        }
    }

    function memoryHelper(uint256 x) internal pure returns (uint256 first, uint256 second) {
        unchecked {
            first = x + 1;
            first *= 3;
            first ^= 5;
            first += 7;
            first *= 11;
            second = x + 2;
        }
    }

    // Void calls expose the unchanged caller prefix at their return label.
    function voidAcross(uint256 x) external returns (uint256) {
        unchecked {
            uint256 keep = x * 3;
            voidHelper();
            return keep + x;
        }
    }

    function voidHelper() internal {
        if (state == 0) {
            state = 1;
        } else {
            state = 2;
        }
    }

    // Nested static calls contribute both return addresses to the physical-stack validation.
    function nestedAcross(uint256 x) external pure returns (uint256) {
        unchecked {
            uint256 keep = x * 3;
            return keep + outerHelper(x);
        }
    }

    function outerHelper(uint256 x) internal pure returns (uint256) {
        unchecked {
            return innerHelper(x) + 3;
        }
    }

    function innerHelper(uint256 x) internal pure returns (uint256 y) {
        unchecked {
            y = x + 1;
            y *= 3;
            y ^= 5;
            y += 7;
            y *= 11;
        }
    }

    // ADDMOD consumes two caller words and the helper result immediately after return.
    function multiAcross(uint256 x) external pure returns (uint256) {
        unchecked {
            uint256 modulus = (x ^ 7) | 1;
            uint256 addend = x * 3;
            return addmod(addend, multiHelper(x), modulus);
        }
    }

    function multiHelper(uint256 x) internal pure returns (uint256 y) {
        unchecked {
            y = x + 1;
            y *= 3;
            y ^= 5;
            y += 7;
            y *= 11;
        }
    }
}
