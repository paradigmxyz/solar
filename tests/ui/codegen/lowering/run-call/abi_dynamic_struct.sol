//@ filecheck:
//@ run-call: WideCalldataEncoding::check; constructor=[32] => 1
// CHECK: @module
//@ codegen-matrix: standard amsterdam
//@[amsterdam] compile-flags: -O gas --evm-version amsterdam
//@ run-call: init (0x000000000000000000000000000000000000beef, 7, "name", 0x0102), 0x0000000000000000000000000000000000000003 => 10
//@ run-call: tail (0x000000000000000000000000000000000000beef, 7, "name", 0x0102) => 0x02
//@ run-call: allocatedAggregates => 2, 18, 5, 2, 8
//@ run-call: fixedAllocatedAggregates => 2, 12, 5, 2, 9
//@ run-call: zeroAllocatedAggregates => 2, 0, 0, 0
//@ run-call: zeroDynamicAggregates => 2, 0, 2, 0
//@ run-call: zeroDynamicAggregatesAfterScratch => 0, 1
//@ run-call: zeroNestedDynamicAggregateAfterScratch => 0

struct InitInput {
    address asset;
    uint8 decimals;
    string name;
    bytes params;
}

struct Allocated {
    uint256 id;
    bytes data;
    uint256[] values;
}

contract AbiDynamicStruct {
    function init(InitInput calldata input, address sink) external pure returns (uint256) {
        return input.decimals + uint160(sink);
    }

    function tail(InitInput calldata input) external pure returns (bytes memory) {
        return input.params[1:];
    }

    function allocatedAggregates()
        external
        pure
        returns (uint256, uint256, uint256, uint256, uint256)
    {
        Allocated[] memory values = new Allocated[](2);
        values[0].id = 7;
        values[0].data = hex"0102";
        values[0].values = new uint256[](2);
        values[0].values[0] = 3;
        values[0].values[1] = 4;
        values[1].id = 11;
        values[1].data = hex"030405";
        values[1].values = new uint256[](1);
        values[1].values[0] = 8;
        return (
            values.length,
            values[0].id + values[1].id,
            values[0].data.length + values[1].data.length,
            values[0].values.length,
            values[1].values[0]
        );
    }

    function fixedAllocatedAggregates()
        external
        pure
        returns (uint256, uint256, uint256, uint256, uint256)
    {
        Allocated[2] memory values;
        values[0].id = 5;
        values[0].data = hex"0102";
        values[0].values = new uint256[](1);
        values[0].values[0] = 6;
        values[1].id = 7;
        values[1].data = hex"030405";
        values[1].values = new uint256[](2);
        values[1].values[0] = 8;
        values[1].values[1] = 9;
        return (
            values.length,
            values[0].id + values[1].id,
            values[0].data.length + values[1].data.length,
            values[1].values.length,
            values[1].values[1]
        );
    }

    function zeroAllocatedAggregates()
        external
        pure
        returns (uint256, uint256, uint256, uint256)
    {
        Allocated[] memory values = new Allocated[](2);
        return (
            values.length,
            values[0].id + values[1].id,
            values[0].data.length + values[1].data.length,
            values[0].values.length + values[1].values.length
        );
    }

    function zeroDynamicAggregates()
        external
        pure
        returns (uint256, uint256, uint256, uint256)
    {
        bytes[] memory bytesValues = new bytes[](2);
        uint256[][] memory arrayValues = new uint256[][](2);
        return (
            bytesValues.length,
            bytesValues[0].length + bytesValues[1].length,
            arrayValues.length,
            arrayValues[0].length + arrayValues[1].length
        );
    }

    function zeroDynamicAggregatesAfterScratch() external pure returns (uint256, uint256) {
        bytes[] memory values = new bytes[](1);
        assembly {
            mstore(0, 99)
        }
        return (values[0].length, values.length);
    }

    function zeroNestedDynamicAggregateAfterScratch() external pure returns (uint256) {
        Allocated[] memory values = new Allocated[](1);
        assembly {
            mstore(0, 99)
        }
        return values[0].data.length;
    }
}

contract WideCalldataEncoding {
    struct Config {
        string name;
        string symbol;
        bytes32 seasonId;
        string seasonName;
        string collectionColor;
        string textColor;
        uint256 roundId;
        uint256 maxSupply;
        uint256 mintPrice;
        uint256 mintDeadline;
        address initialOwner;
        address vrfCoordinator;
        bytes32 keyHash;
        uint16 requestConfirmations;
        uint32 callbackGasLimit;
        uint256 maxAffiliateSlots;
        address enrollmentSigner;
        uint256 prizeBps;
        uint256 affiliatePoolBps;
        address affiliateEligibility;
        uint256 winnerCount;
        uint256 minAffiliateReferrals;
        uint256 affiliatePayoutCapBps;
        uint256 saleStartAt;
    }
    address public immutable factory;
    address public immutable codePart1;
    address public immutable codePart2;
    uint256 private immutable firstLength;
    uint256 private immutable secondLength;
    constructor(uint256 size) {
        factory = address(this);
        codePart1 = msg.sender;
        codePart2 = msg.sender;
        firstLength = size;
        secondLength = size;
    }
    error OnlyFactory();
    error DeploymentFailed();
    function deploy(Config calldata config, address renderer) external returns (address round) {
        if (msg.sender != factory) revert OnlyFactory();
        bytes memory code = new bytes(firstLength + secondLength);
        address part1 = codePart1;
        address part2 = codePart2;
        uint256 firstSize = firstLength;
        uint256 secondSize = secondLength;
        assembly ("memory-safe") {
            extcodecopy(part1, add(code, 32), 1, firstSize)
            extcodecopy(part2, add(add(code, 32), firstSize), 1, secondSize)
        }
        bytes memory init = abi.encodePacked(code, abi.encode(config, renderer));
        assembly ("memory-safe") { round := create(0, add(init, 32), mload(init)) }
        if (round == address(0)) revert DeploymentFailed();
    }

    function check() external returns (uint256) {
        Config memory config;
        config.name = "cat";
        config.symbol = "CAT";
        config.seasonName = "one";
        config.collectionColor = "red";
        config.textColor = "blue";
        config.roundId = 7;
        config.prizeBps = 11;
        config.saleStartAt = 13;
        return this.deploy(config, address(17)) == address(0) ? 0 : 1;
    }
}
