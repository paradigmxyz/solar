//@ codegen-matrix: standard
//@ run-call: scheduleBatch [0x0000000000000000000000000000000000000001], [7], [0x0102], 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000001, 1 => 0x5df7ebed276b3d04ec09631ea1817e7377ea9c5769291856d02865644472f7f1
//@ run-call-fail: scheduleBatch [0x0000000000000000000000000000000000000001], [7], [0x0102], 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000001, 0 => 0x48b6d3db00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001

// A static internal call whose computed calldata-slice arguments are not
// resident when the call is planned: the backend materializes such arguments
// only to give them a spill slot and must drop the transient stack copy again,
// or the caller-stack plan built from the earlier model is applied to a deeper
// stack. Reduced from OpenZeppelin's `TimelockController.scheduleBatch`.
contract StaticCallTransientArgs {
    error InvalidOperationLength(uint256 targets, uint256 payloads, uint256 values);
    error UnexpectedOperationState(bytes32 id, bytes32 expected);
    error InsufficientDelay(uint256 delay, uint256 minDelay);
    error MissingRole(address account, bytes32 role);

    event CallScheduled(bytes32 indexed id, uint256 indexed index, address target, uint256 value, bytes data, bytes32 predecessor, uint256 delay);
    event CallSalt(bytes32 indexed id, bytes32 salt);

    bytes32 public constant PROPOSER_ROLE = keccak256("PROPOSER_ROLE");
    mapping(bytes32 => mapping(address => bool)) private roles;
    mapping(bytes32 => uint256) private timestamps;
    uint256 private minDelay;

    constructor() {
        roles[PROPOSER_ROLE][msg.sender] = true;
        minDelay = 1;
    }

    modifier onlyRole(bytes32 role) {
        if (!roles[role][msg.sender]) revert MissingRole(msg.sender, role);
        _;
    }

    function getMinDelay() public view returns (uint256) {
        return minDelay;
    }

    function isOperation(bytes32 id) public view returns (bool) {
        return timestamps[id] > 0;
    }

    function hashOperationBatch(
        address[] calldata targets,
        uint256[] calldata values,
        bytes[] calldata payloads,
        bytes32 predecessor,
        bytes32 salt
    ) public pure returns (bytes32) {
        return keccak256(abi.encode(targets, values, payloads, predecessor, salt));
    }

    function scheduleBatch(
        address[] calldata targets,
        uint256[] calldata values,
        bytes[] calldata payloads,
        bytes32 predecessor,
        bytes32 salt,
        uint256 delay
    ) public onlyRole(PROPOSER_ROLE) returns (bytes32 id) {
        if (targets.length != values.length || targets.length != payloads.length) {
            revert InvalidOperationLength(targets.length, payloads.length, values.length);
        }
        id = hashOperationBatch(targets, values, payloads, predecessor, salt);
        _schedule(id, delay);
        for (uint256 i = 0; i < targets.length; ++i) {
            emit CallScheduled(id, i, targets[i], values[i], payloads[i], predecessor, delay);
        }
        if (salt != bytes32(0)) {
            emit CallSalt(id, salt);
        }
    }

    function _schedule(bytes32 id, uint256 delay) private {
        if (isOperation(id)) {
            revert UnexpectedOperationState(id, bytes32(uint256(1)));
        }
        uint256 delayFloor = getMinDelay();
        if (delay < delayFloor) {
            revert InsufficientDelay(delay, delayFloor);
        }
        timestamps[id] = block.timestamp + delay;
    }
}
