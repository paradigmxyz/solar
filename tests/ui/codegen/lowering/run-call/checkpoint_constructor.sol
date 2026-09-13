//@ codegen-matrix: standard
//@ run-call: CheckpointConstructor::deploy => 5090
//@ run-call: CheckpointConstructor::multiReturn => 18
//@ run-call: CheckpointConstructor::lookup => 4660

library PackedCheckpoints {
    struct Trace {
        Checkpoint[] checkpoints;
    }

    struct Checkpoint {
        uint96 key;
        uint160 value;
    }

    function push(Trace storage self, uint96 key, uint160 value) internal {
        uint256 len = self.checkpoints.length;
        if (len != 0) {
            Checkpoint storage last = unsafeAccess(self.checkpoints, len - 1);
            require(last.key <= key);
        }
        self.checkpoints.push(Checkpoint(key, value));
    }

    function latest(Trace storage self) internal view returns (bool exists, uint96 key, uint160 value) {
        uint256 len = self.checkpoints.length;
        if (len != 0) {
            Checkpoint storage last = unsafeAccess(self.checkpoints, len - 1);
            return (true, last.key, last.value);
        }
    }

    function lowerLookup(Trace storage self, uint96 key) internal view returns (uint160) {
        uint256 length = self.checkpoints.length;
        uint256 index = lowerBinaryLookup(self.checkpoints, key, 0, length);
        return index == length ? 0 : unsafeAccess(self.checkpoints, index).value;
    }

    function lowerBinaryLookup(Checkpoint[] storage self, uint96 key, uint256 low, uint256 high)
        private view returns (uint256)
    {
        while (low < high) {
            uint256 mid = (low & high) + (low ^ high) / 2;
            if (unsafeAccess(self, mid).key < key) {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        return high;
    }

    function unsafeAccess(Checkpoint[] storage self, uint256 index)
        private
        pure
        returns (Checkpoint storage result)
    {
        assembly {
            mstore(0x00, self.slot)
            result.slot := add(keccak256(0x00, 0x20), index)
        }
    }
}

contract CheckpointTarget {
    using PackedCheckpoints for PackedCheckpoints.Trace;

    event ConsecutiveTransfer(uint256 indexed fromTokenId, uint256 toTokenId, address indexed to);

    uint256[6] private unused;
    PackedCheckpoints.Trace private checkpoints;
    PackedCheckpoints.Trace private otherCheckpoints;
    uint96 private immutable offset;

    constructor(uint96[] memory batches, address receiver, uint96 startingId) {
        offset = startingId;
        for (uint256 i; i < batches.length; ++i) {
            mint(receiver, batches[i]);
        }
        assembly { mstore(0x40, 0x80) }
    }

    function mint(address receiver, uint96 batch) private returns (uint96 next) {
        next = nextId();
        uint96 last = next + batch - 1;
        checkpoints.push(last, uint160(receiver));
        emit ConsecutiveTransfer(next, last, receiver);
    }

    function nextId() private view returns (uint96) {
        (bool exists, uint96 key,) = checkpoints.latest();
        return exists ? key + 1 : offset;
    }

    function latestKey() external view returns (uint96 key) {
        (, key,) = checkpoints.latest();
    }

    function lookup(uint96 key, bool useOther) external view returns (uint160) {
        PackedCheckpoints.Trace storage selected = useOther ? otherCheckpoints : checkpoints;
        return selected.lowerLookup(key);
    }

    function next() external view returns (uint96) {
        return nextId();
    }
}

contract CheckpointConstructor {
    function deploy() external returns (uint96) {
        uint96[] memory batches = new uint96[](2);
        batches[0] = 3922;
        batches[1] = 6;
        return new CheckpointTarget(batches, address(0x1234), 1163).latestKey();
    }

    function lookup() external returns (uint160) {
        uint96[] memory batches = new uint96[](2);
        batches[0] = 3922;
        batches[1] = 6;
        CheckpointTarget target = new CheckpointTarget(batches, address(0x1234), 1163);
        require(target.lookup(5091, false) == 0);
        require(target.lookup(5085, true) == 0);
        return target.lookup(5085, false);
    }

    function multiReturn() external returns (uint8) {
        return new ConstructorMultiReturn().value();
    }
}

contract ConstructorMultiReturn {
    uint8 public immutable value;

    constructor() {
        (, value) = pair();
    }

    function pair() private pure returns (bool, uint8) {
        return (true, 18);
    }
}
