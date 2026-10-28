//@ revisions: none gas size
//@[none] compile-flags: -Onone
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: constructorHash; constructor=[0x0000000000000000000000000000000000000004, [0x0102, 0x030405], 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000000000000000000000000000004] => 0x27cd7cf083f1abb3f3dd0d83e102acc0b5685bc7df11b898f304c8812c7ab40d
//@ run-call: constructorHash; constructor=[0x0000000000000000000000000000000000000004, [], 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000000000000000000000000000004] => 0x569e75fc77c1a856f6daaf9e69d8a9566ca34aa47f9133711ce065a571af0cfd
//@ run-call: computed 7, 4096; constructor=[0x0000000000000000000000000000000000000004, [0x0102, 0x030405], 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000000000000000000000000000004] => 29
//@ run-call: computed 7, 0; constructor=[0x0000000000000000000000000000000000000004, [0x0102, 0x030405], 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000000000000000000000000000004] => 21
//@ run-call: createPair; constructor=[0x0000000000000000000000000000000000000004, [0x0102, 0x030405], 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000000000000000000000000000004] => 17, 100, 200, 4660
//@ run-call: createSingle; constructor=[0x0000000000000000000000000000000000000004, [0x0102, 0x030405], 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000000000000000000000000000004] => 5, 1, 2, 153
//@ run-call: resize 3; constructor=[0x0000000000000000000000000000000000000004, [0x0102, 0x030405], 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000000000000000000000000000004] => 3, 17, 0
//@ run-call: resize 1; constructor=[0x0000000000000000000000000000000000000004, [0x0102, 0x030405], 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000000000000000000000000000004] => 1, 17, 99
//@ run-call: ForwardingBufferLaterFmp::copySelf 0; constructor=[0x0000000000000000000000000000000000000004, [0x0102, 0x030405], 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000000000000000000000000000004] => true
//@ run-call: ForwardingBufferLaterFmp::copySelf 3; constructor=[0x0000000000000000000000000000000000000004, [0x0102, 0x030405], 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000000000000000000000000000004] => true

// A variable-length copy into low memory requires spilled values live
// across it to remain on the stack. The creation payload's free-memory-pointer load
// comes after the copy in the same block; its slot is reserved before its
// definition stores it, so the copy must not reload it as a live value.
contract Child {
    uint256 public start;
    uint256 public b0;
    uint256 public b1;
    address public r0;

    constructor(address[] memory receivers, uint256[] memory batches, uint256 startingId) {
        start = startingId;
        b0 = batches[0];
        b1 = batches[1];
        r0 = receivers[0];
    }
}

contract ForwardingBufferLaterFmp {
    bytes32 public constructorHash;

    constructor(address target, bytes[] memory queries, address factory, bytes memory factoryData) {
        assembly {
            let m := mload(0x40)
            if iszero(extcodesize(target)) {
                if iszero(call(gas(), factory, 0, add(factoryData, 0x20), mload(factoryData), m, 0x20)) {
                    returndatacopy(m, 0, returndatasize())
                    revert(m, returndatasize())
                }
                if iszero(and(gt(returndatasize(), 0x1f), eq(mload(m), target))) { revert(0, 0) }
            }
            let length := mload(queries)
            let n := shl(5, length)
            let r := add(m, 0x40)
            let o := add(r, n)
            for { let i := 0 } iszero(eq(i, n)) { i := add(0x20, i) } {
                let query := mload(add(add(queries, 0x20), i))
                if iszero(call(gas(), target, 0, add(query, 0x20), mload(query), codesize(), 0)) {
                    returndatacopy(m, 0, returndatasize())
                    revert(m, returndatasize())
                }
                mstore(add(r, i), sub(o, r))
                mstore(o, returndatasize())
                returndatacopy(add(o, 0x20), 0, returndatasize())
                o := and(add(add(o, returndatasize()), 0x3f), not(0x1f))
            }
            mstore(m, 0x20)
            mstore(add(m, 0x20), length)
            sstore(constructorHash.slot, keccak256(m, sub(o, m)))
            mstore(0x40, o)
        }
    }

    function one(address account) internal pure returns (address[] memory accounts) {
        accounts = new address[](1);
        accounts[0] = account;
    }

    function createPair() external returns (uint256, uint256, uint256, uint256) {
        address[] memory receivers = new address[](2);
        receivers[0] = address(0x1234);
        receivers[1] = address(0x5678);
        uint256[] memory batches = new uint256[](2);
        batches[0] = 100;
        batches[1] = 200;
        assembly {
            returndatacopy(0, 0, returndatasize())
        }
        Child token = new Child(receivers, batches, 17);
        return (token.start(), token.b0(), token.b1(), uint160(token.r0()));
    }

    function createSingle() external returns (uint256, uint256, uint256, uint256) {
        uint256[] memory batches = new uint256[](2);
        batches[0] = 1;
        batches[1] = 2;
        assembly {
            returndatacopy(0, 0, returndatasize())
        }
        Child token = new Child(one(address(0x99)), batches, 5);
        return (token.start(), token.b0(), token.b1(), uint160(token.r0()));
    }

    struct Buffer {
        uint256[] data;
    }

    function resize(uint256 length) external pure returns (uint256, uint256, uint256) {
        Buffer memory buffer = Buffer(new uint256[](32));
        buffer.data[0] = 17;
        buffer.data[2] = 99;
        assembly {
            mstore(mload(buffer), 1)
        }
        resizeBuffer(buffer, length);
        uint256 last;
        assembly {
            last := mload(add(mload(buffer), 0x60))
        }
        return (buffer.data.length, buffer.data[0], last);
    }

    function resizeBuffer(Buffer memory buffer, uint256 length) internal pure {
        reserveBuffer(buffer, length);
        assembly {
            let data := mload(buffer)
            let oldLength := mload(data)
            if iszero(lt(length, oldLength)) {
                calldatacopy(add(data, shl(5, add(1, oldLength))), calldatasize(), shl(5, sub(length, oldLength)))
            }
            mstore(data, length)
        }
    }

    function reserveBuffer(Buffer memory buffer, uint256 length) internal pure {
        require(length <= 32);
        assembly {
            if iszero(mload(buffer)) { revert(0, 0) }
        }
    }
    function computed(uint256 value, uint256 length) external pure returns (uint256) {
        return computedForward(value, length);
    }

    function computedForward(uint256 value, uint256 length) private pure returns (uint256) {
        uint256 saved = value * 3;
        assembly { calldatacopy(0x80, calldatasize(), length) }
        if (length != 0) return consume(value) + saved;
        return saved;
    }

    function consume(uint256 value) private pure returns (uint256) {
        return value + 1;
    }

    function copy(address target, uint256 length) external view returns (uint256 total) {
        uint256 dest;
        assembly {
            dest := mload(0x40)
        }
        total = copyLoop(target, dest, length);
        assembly {
            return(dest, total)
        }
    }

    function copyLoop(address target, uint256 dest, uint256 length) private view returns (uint256 total) {
        assembly {
            let at := dest
            for { let i := 0 } lt(i, length) { i := add(i, 1) } {
                let size := extcodesize(target)
                extcodecopy(target, at, 0, size)
                at := add(at, size)
            }
            total := sub(at, dest)
        }
    }

    function copySelf(uint256 length) external view returns (bool) {
        uint256 dest;
        assembly { dest := mload(0x40) }
        uint256 copied = copyLoop(address(this), dest, length);
        uint256 codeSize = address(this).code.length;
        if (copied != codeSize * length) return false;
        bytes32 expected = address(this).codehash;
        for (uint256 i; i < length; ++i) {
            bytes32 actual;
            assembly { actual := keccak256(add(dest, mul(i, codeSize)), codeSize) }
            if (actual != expected) return false;
        }
        return true;
    }
}
