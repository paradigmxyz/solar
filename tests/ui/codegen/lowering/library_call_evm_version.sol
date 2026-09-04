//@ revisions: homestead tangerineWhistle byzantium
//@[homestead] compile-flags: -O none --evm-version homestead --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@[homestead] filecheck: --check-prefix=HOMESTEAD --implicit-check-not=returndatasize
//@[tangerineWhistle] compile-flags: -O none --evm-version tangerineWhistle --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@[tangerineWhistle] filecheck: --check-prefix=TANGERINE --implicit-check-not=returndatasize
//@[byzantium] compile-flags: -O none --evm-version byzantium --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@[byzantium] filecheck: --check-prefix=BYZANTIUM

library Lib {
    struct Flagged {
        bool set;
        uint256 value;
    }

    function dbl(uint256 x) external pure returns (uint256) {
        return 2 * x;
    }

    function pair(uint256 x) external pure returns (uint256, uint256) {
        return (x, x + 1);
    }

    function arr(uint256 x) external pure returns (uint256[2] memory) {
        return [x, x];
    }

    function flag(uint256 x) external pure returns (bool) {
        return x != 0;
    }

    function flagged(uint256 x) external pure returns (Flagged memory) {
        return Flagged(x != 0, x);
    }

    function total(uint256[] storage a) external view returns (uint256) {
        return a.length;
    }

    function noret(uint256) external pure {}

    function six(bytes memory b)
        external
        pure
        returns (uint256, uint256, uint256, uint256, uint256, uint256)
    {
        return (b.length, 1, 2, 3, 4, 5);
    }
}

contract C {
    using Lib for uint256[];

    uint256[] private nums;

    // Before Byzantium a linked-library call takes its return values out of a static output area
    // overlaying its own input, as solc's `delegatecall(..., in, 32)` does; from Byzantium on they
    // come out of the return data, and the length check there subsumes the code check.
    // HOMESTEAD-LABEL: fn @one
    // HOMESTEAD: [[IN:v[0-9]+]] = slice_ptr
    // HOMESTEAD: extcodesize
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: delegatecall [[FWD]], {{.*}}, [[IN]], {{.*}}, [[IN]], 32
    // HOMESTEAD: mload
    // From EIP-150 on the forwarded gas is capped, so the reserve goes away while the output area
    // and the code check stay.
    // TANGERINE-LABEL: fn @one
    // TANGERINE: [[IN:v[0-9]+]] = slice_ptr
    // TANGERINE: [[GAS:v[0-9]+]] = gas
    // TANGERINE-NOT: sub [[GAS]]
    // TANGERINE: extcodesize
    // TANGERINE: delegatecall [[GAS]], {{.*}}, [[IN]], {{.*}}, [[IN]], 32
    // TANGERINE: mload
    // BYZANTIUM-LABEL: fn @one
    // BYZANTIUM-NOT: extcodesize
    // BYZANTIUM: delegatecall {{.*}}, 0, 0
    // BYZANTIUM: returndatasize
    function one(uint256 x) external pure returns (uint256) {
        return Lib.dbl(x);
    }

    // Two words overlay the input buffer as well, and the word above the area is touched before
    // the arguments are encoded so that the delegatecall's own memory expansion is not charged
    // against the gas it withholds.
    // HOMESTEAD-LABEL: fn @two
    // HOMESTEAD: [[AREA:v[0-9]+]] = fmp
    // HOMESTEAD: [[ABOVE:v[0-9]+]] = add [[AREA]], 64
    // HOMESTEAD: mstore [[ABOVE]], 0
    // HOMESTEAD: [[IN:v[0-9]+]] = slice_ptr
    // HOMESTEAD-NOT: mstore
    // HOMESTEAD: delegatecall {{.*}}, [[IN]], 64
    // HOMESTEAD: mload
    // HOMESTEAD: mload
    // TANGERINE-LABEL: fn @two
    // TANGERINE: [[IN:v[0-9]+]] = slice_ptr
    // TANGERINE-NOT: alloc raw
    // TANGERINE: delegatecall {{.*}}, [[IN]], 64
    // BYZANTIUM-LABEL: fn @two
    // BYZANTIUM-NOT: extcodesize
    // BYZANTIUM: delegatecall {{.*}}, 0, 0
    // BYZANTIUM: returndatasize
    function two(uint256 x) external pure returns (uint256 a, uint256 b) {
        (a, b) = Lib.pair(x);
    }

    // A statically encoded aggregate is copied out of the overlaid output area into a buffer taken
    // before the arguments, which the decoding then reads.
    // HOMESTEAD-LABEL: fn @aggregate
    // HOMESTEAD: [[BUF:v[0-9]+]] = alloc memorybytes
    // HOMESTEAD: [[DATA:v[0-9]+]] = memory_object_data memorybytes, [[BUF]]
    // HOMESTEAD: [[IN:v[0-9]+]] = slice_ptr
    // HOMESTEAD: delegatecall {{.*}}, [[IN]], 64
    // HOMESTEAD: mcopy [[DATA]], [[IN]], 64
    // HOMESTEAD: abi_decode {{.*}}, [[BUF]]
    // BYZANTIUM-LABEL: fn @aggregate
    // BYZANTIUM-NOT: extcodesize
    // BYZANTIUM: delegatecall {{.*}}, 0, 0
    // BYZANTIUM: returndatasize
    function aggregate(uint256 x) external pure returns (uint256[2] memory) {
        return Lib.arr(x);
    }

    // A returned word is validated where it is read from, so a dirty `bool` reverts before
    // Byzantium as well.
    // HOMESTEAD-LABEL: fn @boolean
    // HOMESTEAD: [[IN:v[0-9]+]] = slice_ptr
    // HOMESTEAD: delegatecall {{.*}}, [[IN]], 32
    // HOMESTEAD: [[WORD:v[0-9]+]] = mload
    // HOMESTEAD: [[CLEAN:v[0-9]+]] = eq [[WORD]],
    // HOMESTEAD: iszero [[CLEAN]]
    // HOMESTEAD: revert 0, 0
    // BYZANTIUM-LABEL: fn @boolean
    // BYZANTIUM: delegatecall {{.*}}, 0, 0
    // BYZANTIUM: returndatasize
    function boolean(uint256 x) external pure returns (bool) {
        return Lib.flag(x);
    }

    // A static struct is decoded out of the output area with its member types, so its `bool` member
    // is validated too.
    // HOMESTEAD-LABEL: fn @structBool
    // HOMESTEAD: [[BUF:v[0-9]+]] = alloc memorybytes
    // HOMESTEAD: [[DATA:v[0-9]+]] = memory_object_data memorybytes, [[BUF]]
    // HOMESTEAD: [[IN:v[0-9]+]] = slice_ptr
    // HOMESTEAD: delegatecall {{.*}}, [[IN]], 64
    // HOMESTEAD: mcopy [[DATA]], [[IN]], 64
    // HOMESTEAD: abi_decode [tuple<bool, u256>], [[BUF]]
    // BYZANTIUM-LABEL: fn @structBool
    // BYZANTIUM: delegatecall {{.*}}, 0, 0
    // BYZANTIUM: returndatasize
    function structBool(uint256 x) external pure returns (Lib.Flagged memory) {
        return Lib.flagged(x);
    }

    // An attached call passes its storage receiver as a slot and reads its return value out of the
    // same overlaid output area.
    // HOMESTEAD-LABEL: fn @attached
    // HOMESTEAD: abi_encode {{.*}}, args 0
    // HOMESTEAD: [[IN:v[0-9]+]] = slice_ptr
    // HOMESTEAD: extcodesize
    // HOMESTEAD: delegatecall {{.*}}, [[IN]], 32
    // HOMESTEAD: mload
    // BYZANTIUM-LABEL: fn @attached
    // BYZANTIUM: abi_encode {{.*}}, args 0
    // BYZANTIUM: delegatecall {{.*}}, 0, 0
    // BYZANTIUM: returndatasize
    function attached() external view returns (uint256) {
        return nums.total();
    }

    // A six-word output area behind a dynamically encoded argument reaches far past the
    // argument's last word, and `sub(gas(), 50)` leaves only two words of expansion. The touch
    // sits at the free-memory pointer plus the output size, so it covers the area whatever the
    // argument encodes to, and it precedes the encoding it would otherwise write over.
    // HOMESTEAD-LABEL: fn @dynamicArgument
    // HOMESTEAD: [[AREA:v[0-9]+]] = fmp
    // HOMESTEAD: [[ABOVE:v[0-9]+]] = add [[AREA]], 192
    // HOMESTEAD: mstore [[ABOVE]], 0
    // HOMESTEAD: [[IN:v[0-9]+]] = slice_ptr
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: delegatecall [[FWD]], {{.*}}, [[IN]], {{.*}}, [[IN]], 192
    // TANGERINE-LABEL: fn @dynamicArgument
    // TANGERINE-NOT: = fmp
    // TANGERINE: delegatecall {{.*}}, [[IN:v[0-9]+]], {{.*}}, [[IN]], 192
    // BYZANTIUM-LABEL: fn @dynamicArgument
    // BYZANTIUM: delegatecall {{.*}}, 0, 0
    // BYZANTIUM: returndatasize
    function dynamicArgument(bytes memory b) external pure returns (uint256, uint256) {
        (uint256 a,,,,, uint256 f) = Lib.six(b);
        return (a, f);
    }

    // A call with no return values declares an empty output area at either version.
    // HOMESTEAD-LABEL: fn @none
    // HOMESTEAD: [[IN:v[0-9]+]] = slice_ptr
    // HOMESTEAD: extcodesize
    // HOMESTEAD: delegatecall {{.*}}, [[IN]], 0
    // BYZANTIUM-LABEL: fn @none
    // BYZANTIUM: extcodesize
    // BYZANTIUM: delegatecall {{.*}}, 0, 0
    function none(uint256 x) external pure {
        Lib.noret(x);
    }
}
