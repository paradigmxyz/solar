//@ revisions: default none gas size byzantium
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@[byzantium] compile-flags: --evm-version byzantium
//@ run-call: tiny; constructor=[171, -1234, 0x000000000000000000000000000000000000beef, 48879, true] => 171
//@ run-call: reassigned; constructor=[171, -1234, 0x000000000000000000000000000000000000beef, 48879, true] => 172
//@ run-call: observedBeforeReassignment; constructor=[171, -1234, 0x000000000000000000000000000000000000beef, 48879, true] => 171
//@ run-call: signed; constructor=[171, -1234, 0x000000000000000000000000000000000000beef, 48879, true] => -1234
//@ run-call: fixedBytes; constructor=[171, -1234, 0x000000000000000000000000000000000000beef, 48879, true] => 0xabcdef
//@ run-call: account; constructor=[171, -1234, 0x000000000000000000000000000000000000beef, 48879, true] => 0x000000000000000000000000000000000000beef
//@ run-call: userDefined; constructor=[171, -1234, 0x000000000000000000000000000000000000beef, 48879, true] => 48879
//@ run-call: flag; constructor=[171, -1234, 0x000000000000000000000000000000000000beef, 48879, true] => true
//@ run-call: callFunctionPointer; constructor=[171, -1234, 0x000000000000000000000000000000000000beef, 48879, true] => 7
//@ run-call: OneByteImmutables::read; constructor=[171, -5, 0xab] => 171, -5, 0xab
//@ run-call: SyntheticImmutableFrame::marker => 77

//@ run-call: ImmutableSourceMemory::read; constructor=[77] => 77, 77, 78, 0

//@ run-call: ImmutableBranchValues::read; constructor=[false, 0] => 20, 20, 0, 0, false, 0x0000000000000000000000000000000000000000, 0x000000
//@ run-call: ImmutableBranchValues::read; constructor=[true, 2] => 12, 10, 0, 0, false, 0x0000000000000000000000000000000000000000, 0x000000

type Tiny is uint16;

contract ImmutableArgs {
    uint8 public immutable tiny;
    uint8 public immutable reassigned;
    int16 public immutable signed;
    bytes3 public immutable fixedBytes = bytes3(uint24(0xABCDEF));
    address public immutable account;
    Tiny public immutable userDefined;
    bool public immutable flag;
    function() internal pure returns (uint256) immutable functionPointer = immutableTarget;
    uint8 public observedBeforeReassignment;

    constructor(uint8 tiny_, int16 signed_, address account_, Tiny userDefined_, bool flag_) {
        tiny = tiny_;
        reassigned = tiny_;
        uint8 previous = reassigned;
        reassigned = tiny_ + 1;
        observedBeforeReassignment = previous;
        signed = signed_;
        account = account_;
        userDefined = userDefined_;
        flag = flag_;
    }

    function callFunctionPointer() external view returns (uint256) {
        return functionPointer();
    }

    function immutableTarget() internal pure returns (uint256) {
        return 7;
    }
}

contract OneByteImmutables {
    uint8 immutable unsignedValue;
    int8 immutable signedValue;
    bytes1 immutable fixedBytesValue;

    constructor(uint8 unsignedValue_, int8 signedValue_, bytes1 fixedBytesValue_) {
        unsignedValue = unsignedValue_;
        signedValue = signedValue_;
        fixedBytesValue = fixedBytesValue_;
    }

    function read() external view returns (uint8, int8, bytes1) {
        return (unsignedValue, signedValue, fixedBytesValue);
    }
}

contract SyntheticFrameBase {
    uint256 public sink;

    constructor() {
        uint256 first;
        uint256 second;
        uint256 third;
        first = 11;
        second = 22;
        third = 33;
        sink = first + second + third;
    }
}

contract SyntheticImmutableFrame is SyntheticFrameBase {
    uint256 public immutable marker = 77;
}

contract ImmutableSourceMemory {
    uint256 public immutable first;
    uint256 public immutable duplicate;
    uint256 public immutable next;
    uint256 public observed;

    constructor(uint256 value) {
        first = value;
        duplicate = first;
        next = readNext();
        uint256 word;
        assembly {
            word := mload(0xc0)
            mstore(0xc0, 0xdead)
            mstore(0xe0, 0xbeef)
            mstore(0x100, 0xbad)
        }
        observed = word;
    }

    function readNext() internal view returns (uint256) {
        return duplicate + 1;
    }

    function read() external view returns (uint256, uint256, uint256, uint256) {
        return (first, duplicate, next, observed);
    }
}

contract ImmutableBranchValues {
    uint256 immutable value;
    uint256 immutable initial;
    uint256 immutable defaultValue;
    bool immutable defaultFlag;
    address immutable defaultAddress;
    bytes3 immutable defaultBytes;
    uint256 beforeAssignment;

    constructor(bool branch, uint256 rounds) {
        beforeAssignment = value;
        if (branch) value = 10;
        else value = 20;
        initial = value;
        for (uint256 i; i < rounds; ++i) value = value + 1;
        uint256[] memory scratch = new uint256[](1);
        assembly { mstore(add(scratch, 32), 42) }
    }

    function read() external view returns (uint256, uint256, uint256, uint256, bool, address, bytes3) {
        return (value, initial, beforeAssignment, defaultValue, defaultFlag, defaultAddress, defaultBytes);
    }
}
