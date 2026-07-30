//@ revisions: default byzantium
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
