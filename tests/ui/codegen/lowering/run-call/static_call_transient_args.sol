//@ codegen-matrix: standard amsterdam
//@[amsterdam] compile-flags: -O gas --evm-version amsterdam
//@ run-call: scheduleBatch [0x0000000000000000000000000000000000000001], [7], [0x0102], 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000001, 1 => 0x5df7ebed276b3d04ec09631ea1817e7377ea9c5769291856d02865644472f7f1
//@ run-call-fail: scheduleBatch [0x0000000000000000000000000000000000000001], [7], [0x0102], 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000001, 0 => 0x48b6d3db00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001

//@ run-call: DeepCallArguments::check 3 => 600
//@ run-call: DeepCallArguments::wide 3 => 5253

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

contract DeepCallArguments {
    function wide(uint256 x) external pure returns (uint256) {
        unchecked {
            uint256 a0 = x + 0;
            uint256 a1 = x + 1;
            uint256 a2 = x + 2;
            uint256 a3 = x + 3;
            uint256 a4 = x + 4;
            uint256 a5 = x + 5;
            uint256 a6 = x + 6;
            uint256 a7 = x + 7;
            uint256 a8 = x + 8;
            uint256 a9 = x + 9;
            uint256 a10 = x + 10;
            uint256 a11 = x + 11;
            uint256 a12 = x + 12;
            uint256 a13 = x + 13;
            uint256 a14 = x + 14;
            uint256 a15 = x + 15;
            uint256 a16 = x + 16;
            uint256 a17 = x + 17;
            uint256 a18 = x + 18;
            uint256 a19 = x + 19;
            return combineWide(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17, a18, a19)
                + combineWide(a19, a18, a17, a16, a15, a14, a13, a12, a11, a10, a9, a8, a7, a6, a5, a4, a3, a2, a1, a0) + x;
        }
    }

    function combineWide(uint256 a0, uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10, uint256 a11, uint256 a12, uint256 a13, uint256 a14, uint256 a15, uint256 a16, uint256 a17, uint256 a18, uint256 a19)
        internal pure returns (uint256)
    {
        unchecked {
            return 1 * a0 + 2 * a1 + 3 * a2 + 4 * a3 + 5 * a4 + 6 * a5 + 7 * a6 + 8 * a7 + 9 * a8 + 10 * a9 + 11 * a10 + 12 * a11 + 13 * a12 + 14 * a13 + 15 * a14 + 16 * a15 + 17 * a16 + 18 * a17 + 19 * a18 + 20 * a19;
        }
    }


    function check(uint256 x) external pure returns (uint256) {
        uint256 a = x + 1;
        uint256 b = x + 2;
        uint256 c = x + 3;
        uint256 d = x + 4;
        uint256 e = x + 5;
        uint256 f = x + 6;
        uint256 g = x + 7;
        uint256 h = x + 8;
        uint256 first = combine(a, b, c, d, e, f, g, h);
        uint256 second = combine(h, g, f, e, d, c, b, a);
        return first + second + a + b + c + d + e + f + g + h;
    }

    function combine(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f, uint256 g, uint256 h)
        internal pure returns (uint256)
    {
        return a + 2 * b + 3 * c + 4 * d + 5 * e + 6 * f + 7 * g + 8 * h;
    }
}
