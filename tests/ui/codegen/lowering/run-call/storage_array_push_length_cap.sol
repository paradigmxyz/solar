//@ codegen-matrix: standard
//@ run-call: grow() => 7, 8, 2
//@ run-call-fail: wrapOntoOwner() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@ run-call-fail: atCap() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
// A storage array's length cannot reach 2**64 by growing, so a length at or
// above it is forged and `keccak256(array_slot) + old_length` wraps onto
// unrelated storage. `wrapOntoOwner` picks the length that makes the appended
// element land on slot 0, which held `owner`; without the cap the push
// overwrote it and returned. solc caps the old length the same way, with
// `Panic(0x41)`.
contract C {
    uint256 owner = 0xbeef;
    uint256[] a;

    function grow() public returns (uint256, uint256, uint256) {
        a.push(7);
        a.push(8);
        return (a[0], a[1], a.length);
    }

    function wrapOntoOwner() public returns (uint256) {
        // 2**256 - keccak256(1), the data slot of the array at slot 1.
        assembly {
            sstore(1, 0x4ef1d2ad89edf8c4d91132028e8195cdf30bb4b5053d4f8cd260341d4805f30a)
        }
        a.push(7);
        return owner;
    }

    function atCap() public {
        assembly {
            sstore(1, 0x10000000000000000)
        }
        a.push(7);
    }
}
