//@ codegen-matrix: standard
//@ run-call: CheckpointConstructor::deploy => 5090
//@ run-call: CheckpointConstructor::multiReturn => 18

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
    uint96 private immutable offset;

    constructor(uint96[] memory batches, address receiver, uint96 startingId) {
        offset = startingId;
        for (uint256 i; i < batches.length; ++i) {
            mint(receiver, batches[i]);
        }
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
