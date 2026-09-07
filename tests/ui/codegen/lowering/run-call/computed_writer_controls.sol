//@ codegen-matrix: standard
//@ run-call: ComputedSharedRoot::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4096, 4096 => 3735928559, 5034, 1
//@ run-call: ComputedWideLiteral::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4096, 4096 => 3735928559, 28924884796661824959755317662140926845399653888090271035429547
//@ run-call: ComputedMultipleUse::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4096, 4096 => 3735928559, 4779, 257
//@ run-call: ComputedMutableBank::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4096, 4096 => 3735928559, 4522
//@ run-call: ComputedReturning::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4096, 4096, 0 => 3735928559, 4779, 1, true
//@ run-call: ComputedReturning::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4096, 4096, 1 => 3735928559, 4779, 2, true
//@ run-call: ComputedReturning::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4096, 4096, 2 => 3735928559, 4779, 3, true

// The calldata producer is shared by two recipes and a direct output; dead has no consumers.
contract ComputedSharedRoot {
    function run(uint256[18] calldata values, uint256 destination, uint256 source)
        external pure returns (uint256 observed, uint256 checksum, uint256 original)
    {
        assembly {
            let root := calldataload(values)
            let dead := xor(root, 999)
            let a00 := xor(root, 256)
            let a01 := xor(root, 512)
            let a02 := xor(calldataload(add(values, 64)), 256)
            let a03 := xor(calldataload(add(values, 96)), 256)
            let a04 := xor(calldataload(add(values, 128)), 256)
            let a05 := xor(calldataload(add(values, 160)), 256)
            let a06 := xor(calldataload(add(values, 192)), 256)
            let a07 := xor(calldataload(add(values, 224)), 256)
            let a08 := xor(calldataload(add(values, 256)), 256)
            let a09 := xor(calldataload(add(values, 288)), 256)
            let a10 := xor(calldataload(add(values, 320)), 256)
            let a11 := xor(calldataload(add(values, 352)), 256)
            let a12 := xor(calldataload(add(values, 384)), 256)
            let a13 := xor(calldataload(add(values, 416)), 256)
            let a14 := xor(calldataload(add(values, 448)), 256)
            let a15 := xor(calldataload(add(values, 480)), 256)
            let a16 := xor(calldataload(add(values, 512)), 256)
            let a17 := xor(calldataload(add(values, 544)), 256)
            mstore(destination, 0xdeadbeef)
            observed := mload(source)
            original := root
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
        }
    }
}

// Repeated wide literals may share DUP materialization; a recipe must charge actual bytes.
contract ComputedWideLiteral {
    function run(uint256[18] calldata values, uint256 destination, uint256 source)
        external pure returns (uint256 observed, uint256 checksum)
    {
        assembly {
            let a00 := xor(calldataload(add(values, 0)), 0x100000000000000000000000000000000000000000000000100)
            let a01 := xor(calldataload(add(values, 32)), 0x100000000000000000000000000000000000000000000000100)
            let a02 := xor(calldataload(add(values, 64)), 0x100000000000000000000000000000000000000000000000100)
            let a03 := xor(calldataload(add(values, 96)), 0x100000000000000000000000000000000000000000000000100)
            let a04 := xor(calldataload(add(values, 128)), 0x100000000000000000000000000000000000000000000000100)
            let a05 := xor(calldataload(add(values, 160)), 0x100000000000000000000000000000000000000000000000100)
            let a06 := xor(calldataload(add(values, 192)), 0x100000000000000000000000000000000000000000000000100)
            let a07 := xor(calldataload(add(values, 224)), 0x100000000000000000000000000000000000000000000000100)
            let a08 := xor(calldataload(add(values, 256)), 0x100000000000000000000000000000000000000000000000100)
            let a09 := xor(calldataload(add(values, 288)), 0x100000000000000000000000000000000000000000000000100)
            let a10 := xor(calldataload(add(values, 320)), 0x100000000000000000000000000000000000000000000000100)
            let a11 := xor(calldataload(add(values, 352)), 0x100000000000000000000000000000000000000000000000100)
            let a12 := xor(calldataload(add(values, 384)), 0x100000000000000000000000000000000000000000000000100)
            let a13 := xor(calldataload(add(values, 416)), 0x100000000000000000000000000000000000000000000000100)
            let a14 := xor(calldataload(add(values, 448)), 0x100000000000000000000000000000000000000000000000100)
            let a15 := xor(calldataload(add(values, 480)), 0x100000000000000000000000000000000000000000000000100)
            let a16 := xor(calldataload(add(values, 512)), 0x100000000000000000000000000000000000000000000000100)
            let a17 := xor(calldataload(add(values, 544)), 0x100000000000000000000000000000000000000000000000100)
            mstore(destination, 0xdeadbeef)
            observed := mload(source)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
        }
    }
}

// a00 has two independent consumers after the source writer.
contract ComputedMultipleUse {
    function run(uint256[18] calldata values, uint256 destination, uint256 source)
        external pure returns (uint256 observed, uint256 checksum, uint256 again)
    {
        assembly {
            let a00 := xor(calldataload(add(values, 0)), 256)
            let a01 := xor(calldataload(add(values, 32)), 256)
            let a02 := xor(calldataload(add(values, 64)), 256)
            let a03 := xor(calldataload(add(values, 96)), 256)
            let a04 := xor(calldataload(add(values, 128)), 256)
            let a05 := xor(calldataload(add(values, 160)), 256)
            let a06 := xor(calldataload(add(values, 192)), 256)
            let a07 := xor(calldataload(add(values, 224)), 256)
            let a08 := xor(calldataload(add(values, 256)), 256)
            let a09 := xor(calldataload(add(values, 288)), 256)
            let a10 := xor(calldataload(add(values, 320)), 256)
            let a11 := xor(calldataload(add(values, 352)), 256)
            let a12 := xor(calldataload(add(values, 384)), 256)
            let a13 := xor(calldataload(add(values, 416)), 256)
            let a14 := xor(calldataload(add(values, 448)), 256)
            let a15 := xor(calldataload(add(values, 480)), 256)
            let a16 := xor(calldataload(add(values, 512)), 256)
            let a17 := xor(calldataload(add(values, 544)), 256)
            mstore(destination, 0xdeadbeef)
            observed := mload(source)
            again := a00
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
        }
    }
}

// The mutable SLOAD value must remain a real value; gas-mode bank selection cannot split it.
contract ComputedMutableBank {
    function run(uint256[18] calldata values, uint256 destination, uint256 source)
        external view returns (uint256 observed, uint256 checksum)
    {
        assembly {
            let a00 := sload(0)
            let a01 := xor(calldataload(add(values, 32)), 256)
            let a02 := xor(calldataload(add(values, 64)), 256)
            let a03 := xor(calldataload(add(values, 96)), 256)
            let a04 := xor(calldataload(add(values, 128)), 256)
            let a05 := xor(calldataload(add(values, 160)), 256)
            let a06 := xor(calldataload(add(values, 192)), 256)
            let a07 := xor(calldataload(add(values, 224)), 256)
            let a08 := xor(calldataload(add(values, 256)), 256)
            let a09 := xor(calldataload(add(values, 288)), 256)
            let a10 := xor(calldataload(add(values, 320)), 256)
            let a11 := xor(calldataload(add(values, 352)), 256)
            let a12 := xor(calldataload(add(values, 384)), 256)
            let a13 := xor(calldataload(add(values, 416)), 256)
            let a14 := xor(calldataload(add(values, 448)), 256)
            let a15 := xor(calldataload(add(values, 480)), 256)
            let a16 := xor(calldataload(add(values, 512)), 256)
            let a17 := xor(calldataload(add(values, 544)), 256)
            mstore(destination, 0xdeadbeef)
            observed := mload(source)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
        }
    }
}

// Runtime depth prevents assuming a stack-only or inlined call; verify actual ICall/protocol.
contract ComputedReturning {
    function run(uint256[18] calldata values, uint256 destination, uint256 source, uint256 depth)
        external view returns (uint256 observed, uint256 checksum, uint256 steps, bool gasOrdered)
    {
        assembly {
            function recurse(n) -> x {
                if n { x := recurse(sub(n, 1)) }
                x := add(x, 1)
            }
            let a00 := xor(calldataload(add(values, 0)), 256)
            let a01 := xor(calldataload(add(values, 32)), 256)
            let a02 := xor(calldataload(add(values, 64)), 256)
            let a03 := xor(calldataload(add(values, 96)), 256)
            let a04 := xor(calldataload(add(values, 128)), 256)
            let a05 := xor(calldataload(add(values, 160)), 256)
            let a06 := xor(calldataload(add(values, 192)), 256)
            let a07 := xor(calldataload(add(values, 224)), 256)
            let a08 := xor(calldataload(add(values, 256)), 256)
            let a09 := xor(calldataload(add(values, 288)), 256)
            let a10 := xor(calldataload(add(values, 320)), 256)
            let a11 := xor(calldataload(add(values, 352)), 256)
            let a12 := xor(calldataload(add(values, 384)), 256)
            let a13 := xor(calldataload(add(values, 416)), 256)
            let a14 := xor(calldataload(add(values, 448)), 256)
            let a15 := xor(calldataload(add(values, 480)), 256)
            let a16 := xor(calldataload(add(values, 512)), 256)
            let a17 := xor(calldataload(add(values, 544)), 256)
            let beforeGas := gas()
            steps := recurse(depth)
            mstore(destination, 0xdeadbeef)
            observed := mload(source)
            gasOrdered := gt(beforeGas, gas())
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
        }
    }
}
