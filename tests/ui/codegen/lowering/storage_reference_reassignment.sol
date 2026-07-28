//@ run-call: assign true => 11
//@ run-call: assign false => 22
//@ run-call: reassign true => 32
//@ run-call: reassign false => 31

contract StorageReferenceReassignment {
    struct Status {
        uint256 remaining;
    }

    mapping(uint256 => Status) internal statuses;

    function assign(bool second) external returns (uint256) {
        Status storage status;
        if (second) {
            status = statuses[2];
        } else {
            status = statuses[1];
        }
        status.remaining = second ? 11 : 22;
        return statuses[second ? 2 : 1].remaining;
    }

    function reassign(bool second) external returns (uint256) {
        Status storage status = statuses[1];
        if (second) {
            status = statuses[2];
        }
        status.remaining = second ? 32 : 31;
        return statuses[second ? 2 : 1].remaining;
    }
}
