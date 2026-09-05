//@ revisions: default strip debug
//@[strip] compile-flags: --revert-strings strip
//@[debug] compile-flags: --revert-strings debug

//@ run-call: requireMessage 1 => 1
//@[default,debug] run-call-fail: requireMessage 0 => Error("x must be one")
//@[strip] run-call-fail: requireMessage 0 => 0x

//@[default,debug] run-call-fail: requireConstantMessage 0 => Error("constant message")
//@[strip] run-call-fail: requireConstantMessage 0 => 0x

//@[default,debug] run-call-fail: revertMessage => Error("always")
//@[strip] run-call-fail: revertMessage => 0x

//@[default,debug] run-call-fail: revertDynamicMessage 3 => Error("value 3")
//@[strip] run-call-fail: revertDynamicMessage 3 => 0x

//@ run-call: requireSideEffects => 1

//@[default,debug] run-call-fail: revertSideEffects => Error("bumped")
//@[strip] run-call-fail: revertSideEffects => 0x

//@ run-call: storageReason 1 => 1
//@[default,debug] run-call-fail: storageReason 0 => Error("stored reason string that is longer than thirty-two bytes")
//@[strip] run-call-fail: storageReason 0 => 0x

//@ run-call: conversionReason 1 => 1
//@[default,debug] run-call-fail: conversionReason 0 => Error("value 0")
//@[strip] run-call-fail: conversionReason 0 => 0x
//@ run-call-fail: conversionReason 255 => Panic(0x11)

//@ run-call: indexedReason 1, 0 => 1
//@[default,debug] run-call-fail: indexedReason 0, 1 => Error("second")
//@[strip] run-call-fail: indexedReason 0, 1 => 0x
//@ run-call-fail: indexedReason 1, 5 => Panic(0x32)

//@ run-call-fail: dividedReason 1, 0 => Panic(0x12)

//@ run-call: slicedReason 0x0102, 0, 1 => 1
//@[default,strip] run-call-fail: slicedReason 0x0102, 2, 1 => 0x
//@[debug] run-call-fail: slicedReason 0x0102, 2, 1 => Error("Slice starts after end")

// A calldata struct member as the reason runs the lazy tail checks in every mode, so a
// member offset past the end of calldata reverts even when the condition holds.
//@ run-call: memberReason (1, "member"), 1 => 1
//@[default,debug] run-call-fail: memberReason (1, "member"), 0 => Error("member")
//@[strip] run-call-fail: memberReason (1, "member"), 0 => 0x
//@[default,strip] run-call-fail: 0xe6c5a0e30000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000010000000000000000 => 0x
//@[debug] run-call-fail: 0xe6c5a0e30000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000010000000000000000 => Error("Invalid calldata tail offset")

//@ run-call-fail: customError 7 => Custom(uint256)(7)

//@ run-call-fail: requireCustomError 0 => Custom(uint256)(0)

// User-supplied reason strings under each `--revert-strings` mode. `strip` drops the
// payload of `require` and `revert` reason strings but, like solc, still evaluates the
// reason, so its side effects and panics are kept. Custom errors are untouched in every
// mode.
contract UserStrings {
    string constant MESSAGE = "constant message";
    error Custom(uint256 value);

    uint256 bumps;
    string reason = "stored reason string that is longer than thirty-two bytes";
    string[2] messages = ["first", "second"];

    struct S {
        uint256 a;
        string reason;
    }

    function requireMessage(uint256 x) external pure returns (uint256) {
        require(x == 1, "x must be one");
        return x;
    }

    function requireConstantMessage(uint256 x) external pure returns (uint256) {
        require(x == 1, MESSAGE);
        return x;
    }

    function revertMessage() external pure {
        revert("always");
    }

    function revertDynamicMessage(uint256 x) external pure {
        revert(string(abi.encodePacked("value ", bytes1(uint8(x) + 48))));
    }

    function bump() internal returns (string memory) {
        bumps += 1;
        return "bumped";
    }

    function requireSideEffects() external returns (uint256) {
        require(true, bump());
        return bumps;
    }

    function revertSideEffects() external {
        revert(bump());
    }

    function storageReason(uint256 x) external view returns (uint256) {
        require(x == 1, reason);
        return x;
    }

    function conversionReason(uint256 x) external pure returns (uint256) {
        require(x == 1, string(abi.encodePacked("value ", bytes1(uint8(x) + 48))));
        return x;
    }

    function indexedReason(uint256 x, uint256 i) external view returns (uint256) {
        require(x == 1, messages[i]);
        return x;
    }

    function dividedReason(uint256 x, uint256 d) external view returns (uint256) {
        require(x == 1, messages[1 / d]);
        return x;
    }

    function slicedReason(bytes calldata data, uint256 start, uint256 end) external pure returns (uint256) {
        require(start == 0, string(data[start:end]));
        return end;
    }

    function memberReason(S calldata s, uint256 x) external pure returns (uint256) {
        require(x == 1, s.reason);
        return x;
    }

    function customError(uint256 x) external pure {
        revert Custom(x);
    }

    function requireCustomError(uint256 x) external pure returns (uint256) {
        require(x == 1, Custom(x));
        return x;
    }
}
