//@ revisions: homestead spurious byzantium osaka
//@[homestead] compile-flags: -O none --evm-version homestead --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@[homestead] filecheck: --check-prefix=PREBYZ
//@[spurious] compile-flags: -O none --evm-version spuriousDragon --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@[spurious] filecheck: --check-prefix=PREBYZ
//@[byzantium] compile-flags: -O none --evm-version byzantium --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@[byzantium] filecheck: --check-prefix=POSTBYZ
//@[osaka] compile-flags: -O none --evm-version osaka --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@[osaka] filecheck: --check-prefix=POSTBYZ

// A storage reference crossing a library call boundary travels as its slot number in both
// directions, like solc's `ArrayType::encodingType`, so the ABI wrapper returns one word and the
// caller decodes one word. Storage pointers are a single static word, so the call is legal before
// Byzantium too. `run-call` cannot deploy an unlinked library, so this pins the lowering only.
library Lib {
    struct Plain {
        uint256 a;
    }

    // The wrapper receives the slot and returns the slot, never a memory array.
    // PREBYZ: fn @arrRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    // POSTBYZ: fn @arrRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    function arrRef(uint256[] storage a) external pure returns (uint256[] storage) {
        return a;
    }

    // PREBYZ: fn @bytesRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    // POSTBYZ: fn @bytesRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    function bytesRef(bytes storage b) external pure returns (bytes storage) {
        return b;
    }

    // PREBYZ: fn @plainRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    // POSTBYZ: fn @plainRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    function plainRef(Plain storage p) external pure returns (Plain storage) {
        return p;
    }

    // A fixed array and a mapping are slots too.
    // PREBYZ: fn @fixedRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    // POSTBYZ: fn @fixedRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    function fixedRef(uint256[3] storage a) external pure returns (uint256[3] storage) {
        return a;
    }

    // PREBYZ: fn @mapRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    // POSTBYZ: fn @mapRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    function mapRef(mapping(uint256 => uint256) storage m)
        external
        pure
        returns (mapping(uint256 => uint256) storage)
    {
        return m;
    }

    // A slot beside a statically encoded aggregate keeps its one word inside that aggregate's
    // head, so the head is 96 bytes and not the 128 a memory pointer pair would take.
    // PREBYZ: fn @aggRef{{.*}}abi_returns=[word, array<2, word>]
    // POSTBYZ: fn @aggRef{{.*}}abi_returns=[word, array<2, word>]
    function aggRef(Plain storage p) external pure returns (Plain storage, uint256[2] memory) {
        return (p, [uint256(1), uint256(2)]);
    }

    // A slot next to a value keeps the value's own shape.
    // PREBYZ: fn @pairRef{{.*}}abi_returns=[word, word]
    // POSTBYZ: fn @pairRef{{.*}}abi_returns=[word, word]
    function pairRef(uint256[] storage a) external view returns (uint256, uint256[] storage) {
        return (a.length, a);
    }
}

contract C {
    using Lib for uint256[];

    uint256[] private nums;
    bytes private bs;
    Lib.Plain private plain;
    uint256[3] private fixedNums;
    mapping(uint256 => uint256) private byKey;

    // Before Byzantium the returned slot comes out of the delegatecall's own 32-byte output area,
    // as solc's `delegatecall(..., out, 32)` does; from Byzantium on it is decoded out of the
    // return data as a word and used as a slot.
    // PREBYZ-LABEL: fn @len
    // PREBYZ: delegatecall {{.*}}, [[IN:v[0-9]+]], {{v[0-9]+}}, [[IN]], 32
    // PREBYZ: mload
    // PREBYZ: sload
    // POSTBYZ-LABEL: fn @len
    // POSTBYZ: delegatecall {{.*}}, 0, 0
    // POSTBYZ: [[SLOT:v[0-9]+]] = abi_decode [storageptr]
    // POSTBYZ: sload [[SLOT]]
    function len() external view returns (uint256) {
        return Lib.arrRef(nums).length;
    }

    // A `bytes` slot is a slot too: the length still comes from storage.
    // PREBYZ-LABEL: fn @bytesLen
    // PREBYZ: delegatecall {{.*}}, [[IN:v[0-9]+]], {{v[0-9]+}}, [[IN]], 32
    // PREBYZ: mload
    // PREBYZ: sload
    // POSTBYZ-LABEL: fn @bytesLen
    // POSTBYZ: [[SLOT:v[0-9]+]] = abi_decode [storageptr]
    // POSTBYZ: sload [[SLOT]]
    function bytesLen() external view returns (uint256) {
        return Lib.bytesRef(bs).length;
    }

    // A struct pointer indexes its member off the returned slot.
    // PREBYZ-LABEL: fn @member
    // PREBYZ: delegatecall {{.*}}, [[IN:v[0-9]+]], {{v[0-9]+}}, [[IN]], 32
    // PREBYZ: mload
    // PREBYZ: sload
    // POSTBYZ-LABEL: fn @member
    // POSTBYZ: [[SLOT:v[0-9]+]] = abi_decode [storageptr]
    // POSTBYZ: sload [[SLOT]]
    function member() external view returns (uint256) {
        return Lib.plainRef(plain).a;
    }

    // Two return words share one output area, and the second one is the slot.
    // PREBYZ-LABEL: fn @pair
    // PREBYZ: delegatecall {{.*}}, [[IN:v[0-9]+]], {{v[0-9]+}}, [[IN]], 64
    // POSTBYZ-LABEL: fn @pair
    // POSTBYZ: abi_decode [u256, storageptr]
    function pair() external view returns (uint256, uint256) {
        (uint256 n, uint256[] storage r) = Lib.pairRef(nums);
        return (n, r.length);
    }

    // PREBYZ-LABEL: fn @fixedLast
    // PREBYZ: delegatecall {{.*}}, [[IN:v[0-9]+]], {{v[0-9]+}}, [[IN]], 32
    // PREBYZ: mload
    // PREBYZ: sload
    // POSTBYZ-LABEL: fn @fixedLast
    // POSTBYZ: [[SLOT:v[0-9]+]] = abi_decode [storageptr]
    // POSTBYZ: sload
    function fixedLast() external view returns (uint256) {
        return Lib.fixedRef(fixedNums)[2];
    }

    // PREBYZ-LABEL: fn @mapValue
    // PREBYZ: delegatecall {{.*}}, [[IN:v[0-9]+]], {{v[0-9]+}}, [[IN]], 32
    // PREBYZ: [[SLOT:v[0-9]+]] = mload
    // PREBYZ: mapping_slot
    // POSTBYZ-LABEL: fn @mapValue
    // POSTBYZ: [[SLOT:v[0-9]+]] = abi_decode [storageptr]
    // POSTBYZ: mapping_slot {{.*}}, [[SLOT]]
    function mapValue(uint256 key) external view returns (uint256) {
        return Lib.mapRef(byKey)[key];
    }

    // The static aggregate return is decoded out of a 96-byte output area, with the slot as the
    // first word of the head.
    // PREBYZ-LABEL: fn @agg
    // PREBYZ: delegatecall {{.*}}, [[IN:v[0-9]+]], {{v[0-9]+}}, [[IN]], 96
    // PREBYZ: [[SLOT:v[0-9]+]] = abi_decode [storageptr, array<2, u256>]
    // PREBYZ: sload [[SLOT]]
    // POSTBYZ-LABEL: fn @agg
    // POSTBYZ: [[SLOT:v[0-9]+]] = abi_decode [storageptr, array<2, u256>]
    // POSTBYZ: sload [[SLOT]]
    function agg() external view returns (uint256, uint256) {
        (Lib.Plain storage p, uint256[2] memory m) = Lib.aggRef(plain);
        return (p.a, m[1]);
    }

    // An attached call passes the receiver's slot and gets a slot back.
    // PREBYZ-LABEL: fn @attached
    // PREBYZ: delegatecall {{.*}}, [[IN:v[0-9]+]], {{v[0-9]+}}, [[IN]], 32
    // PREBYZ: mload
    // PREBYZ: sload
    // POSTBYZ-LABEL: fn @attached
    // POSTBYZ: [[SLOT:v[0-9]+]] = abi_decode [storageptr]
    // POSTBYZ: sload [[SLOT]]
    function attached() external view returns (uint256) {
        return nums.arrRef().length;
    }
}
