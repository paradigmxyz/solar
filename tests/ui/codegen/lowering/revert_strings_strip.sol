//@ codegen-matrix: standard
//@ compile-flags: --revert-strings strip
//@ run-call: requireMessage 1 => 1
//@ run-call-fail: requireMessage 0 => 0x
//@ run-call-fail: requireConstantMessage 0 => 0x
//@ run-call-fail: revertMessage => 0x
//@ run-call-fail: revertDynamicMessage 3 => 0x
//@ run-call: requireSideEffects => 1
//@ run-call-fail: revertSideEffects => 0x
//@ run-call: storageReason 1 => 1
//@ run-call-fail: storageReason 0 => 0x
//@ run-call: conversionReason 1 => 1
//@ run-call-fail: conversionReason 0 => 0x
//@ run-call-fail: conversionReason 255 => Panic(0x11)
//@ run-call: indexedReason 1, 0 => 1
//@ run-call-fail: indexedReason 0, 1 => 0x
//@ run-call-fail: indexedReason 1, 5 => Panic(0x32)
//@ run-call-fail: dividedReason 1, 0 => Panic(0x12)
//@ run-call: slicedReason 0x0102, 0, 1 => 1
//@ run-call-fail: slicedReason 0x0102, 2, 1 => 0x
//@ run-call-fail: customError 7 => Custom(uint256)(7)
//@ run-call-fail: requireCustomError 0 => Custom(uint256)(0)

// `--revert-strings strip` drops `require` and `revert` reason strings but keeps
// the side effects and failures of evaluating them, like solc, and leaves custom
// errors untouched. Constants and plain variable reads are not lowered at all, so
// `storageReason` never loads or copies `reason` (see the MIR output).
contract RevertStringsStrip {
    string constant MESSAGE = "constant message";
    error Custom(uint256 value);

    uint256 bumps;
    string reason = "stored reason string that is longer than thirty-two bytes";
    string[2] messages = ["first", "second"];

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

    function customError(uint256 x) external pure {
        revert Custom(x);
    }

    function requireCustomError(uint256 x) external pure returns (uint256) {
        require(x == 1, Custom(x));
        return x;
    }
}
