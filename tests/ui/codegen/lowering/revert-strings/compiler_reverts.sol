//@ revisions: default strip debug
//@[strip] compile-flags: --revert-strings strip
//@[debug] compile-flags: --revert-strings debug

// Dispatch: Ether sent to a non-payable function, and a selector matching nothing.
//@ run-call: passthrough 7 => 7
//@[default,strip] run-call-fail: passthrough 7; value=1 => 0x
//@[debug] run-call-fail: passthrough 7; value=1 => Error("Ether sent to non-payable function")
//@[default,strip] run-call-fail: 0xdeadbeef => 0x
//@[debug] run-call-fail: 0xdeadbeef => Error("Contract does not have fallback nor receive functions")

// `passthrough(uint256)` with no argument word after the selector.
//@[default,strip] run-call-fail: 0x8f336a56 => 0x
//@[debug] run-call-fail: 0x8f336a56 => Error("ABI decoding: tuple data too short")

// `flag(bool)` with the non-canonical word 2: validators revert with empty data in every mode.
//@ run-call: flag true => true
//@ run-call-fail: 0xa92a4c3b0000000000000000000000000000000000000000000000000000000000000002 => 0x

// `slice(bytes,uint256,uint256)`: slice bounds, then the `bytes` head offset overflowing 64 bits and pointing past the end of calldata.
//@ run-call: slice 0x01020304, 1, 3 => 2
//@[default,strip] run-call-fail: slice 0x01020304, 0, 5 => 0x
//@[debug] run-call-fail: slice 0x01020304, 0, 5 => Error("Slice is greater than length")
//@[default,strip] run-call-fail: slice 0x01020304, 2, 1 => 0x
//@[debug] run-call-fail: slice 0x01020304, 2, 1 => Error("Slice starts after end")
//@[default,strip] run-call-fail: slice 0x01020304, 6, 5 => 0x
//@[debug] run-call-fail: slice 0x01020304, 6, 5 => Error("Slice starts after end")
//@[default,strip] run-call-fail: 0xe0041396000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001 => 0x
//@[debug] run-call-fail: 0xe0041396000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001 => Error("ABI decoding: invalid tuple offset")
//@[default,strip] run-call-fail: 0xe0041396000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001 => 0x
//@[debug] run-call-fail: 0xe0041396000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001 => Error("ABI decoding: invalid calldata array offset")

// `arr(uint256[])`: offset overflowing 64 bits, head at the end of calldata, length of 2**64, and three elements with no data.
//@[default,strip] run-call-fail: 0xbfceb9770000000000000000000000000000000000000000000000010000000000000000 => 0x
//@[debug] run-call-fail: 0xbfceb9770000000000000000000000000000000000000000000000010000000000000000 => Error("ABI decoding: invalid tuple offset")
//@[default,strip] run-call-fail: 0xbfceb9770000000000000000000000000000000000000000000000000000000000000040 => 0x
//@[debug] run-call-fail: 0xbfceb9770000000000000000000000000000000000000000000000000000000000000040 => Error("ABI decoding: invalid calldata array offset")
//@[default,strip] run-call-fail: 0xbfceb97700000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000010000000000000000 => 0x
//@[debug] run-call-fail: 0xbfceb97700000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000010000000000000000 => Error("ABI decoding: invalid calldata array length")
//@[default,strip] run-call-fail: 0xbfceb97700000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003 => 0x
//@[debug] run-call-fail: 0xbfceb97700000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003 => Error("ABI decoding: invalid calldata array stride")

// `echo(uint256[])` returns its live calldata argument; a length of 2**64 fails while materializing it.
//@ run-call: echo [1, 2] => [1, 2]
//@[default,strip] run-call-fail: 0x751d92270000000000000000000000000000000000000000000000000000000000000020ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x
//@[debug] run-call-fail: 0x751d92270000000000000000000000000000000000000000000000000000000000000020ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => Error("ABI decoding: invalid calldata array length")

// `st((uint256,bytes))` stays in calldata: offset overflowing 64 bits, a struct head cut short, and a member offset that is only validated on access.
//@ run-call: st (1, 0x) => 1
//@[default,strip] run-call-fail: 0xb31061880000000000000000000000000000000000000000000000010000000000000000 => 0x
//@[debug] run-call-fail: 0xb31061880000000000000000000000000000000000000000000000010000000000000000 => Error("ABI decoding: invalid tuple offset")
//@[default,strip] run-call-fail: 0xb31061880000000000000000000000000000000000000000000000000000000000000020 => 0x
//@[debug] run-call-fail: 0xb31061880000000000000000000000000000000000000000000000000000000000000020 => Error("ABI decoding: struct calldata too short")
//@ run-call: 0xb3106188000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000010000000000000000 => 0x0000000000000000000000000000000000000000000000000000000000000001

// `stMem((uint256,bytes))` decodes to memory, so the member offset is checked eagerly.
//@ run-call: stMem (7, 0x) => 7
//@[default,strip] run-call-fail: 0x982896e4000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000010000000000000000 => 0x
//@[debug] run-call-fail: 0x982896e4000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000010000000000000000 => Error("ABI decoding: invalid struct offset")
//@[default,strip] run-call-fail: 0x982896e400000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001 => 0x
//@[debug] run-call-fail: 0x982896e400000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001 => Error("ABI decoding: struct data too short")

// `blob(bytes)` and an unused `bytes memory` parameter with a length past the end of calldata.
//@ run-call: blob 0x0102 => 2
//@[default,strip] run-call-fail: 0x03cd516700000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000064 => 0x
//@[debug] run-call-fail: 0x03cd516700000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000064 => Error("ABI decoding: invalid byte array length")
//@ run-call: unusedBlob 0x0102 => 1
//@[default,strip] run-call-fail: 0x26a4181600000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000064 => 0x
//@[debug] run-call-fail: 0x26a4181600000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000064 => Error("ABI decoding: invalid byte array length")

// `dec(bytes)` runs `abi.decode` on memory data holding only a struct offset.
//@[default,strip] run-call-fail: 0x1a00934d000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000020 => 0x
//@[debug] run-call-fail: 0x1a00934d000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000020 => Error("ABI decoding: struct data too short")

// `fixedBytes(bytes[2])`: the array head at the end of calldata, then element heads that do not fit.
//@ run-call: fixedBytes [0x01, 0x0203] => 3
//@[default,strip] run-call-fail: 0xf526f26e0000000000000000000000000000000000000000000000000000000000000020 => 0x
//@[debug] run-call-fail: 0xf526f26e0000000000000000000000000000000000000000000000000000000000000020 => Error("ABI decoding: invalid calldata array offset")
//@[default,strip] run-call-fail: 0xf526f26e00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000040 => 0x
//@[debug] run-call-fail: 0xf526f26e00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000040 => Error("ABI decoding: invalid calldata array stride")

// External calls to an address without code, with and without return data.
//@[default,strip] run-call-fail: callNoCode => 0x
//@[debug] run-call-fail: callNoCode => Error("Target contract does not contain code")
//@[default,strip] run-call-fail: callNoCodeReturning => 0x
//@[debug] run-call-fail: callNoCodeReturning => Error("Target contract does not contain code")

// User-supplied reason strings are kept by default and in debug mode.
//@[default,debug] run-call-fail: userRevert => Error("user")
//@[strip] run-call-fail: userRevert => 0x
//@[default,debug] run-call-fail: userRequire 0 => Error("x must be nonzero")
//@[strip] run-call-fail: userRequire 0 => 0x

// Compiler-generated reverts under each `--revert-strings` mode. By default and with
// `strip` they carry no data. With `debug` they carry solc's `Error(string)` messages:
// rejected Ether, unknown selectors, malformed ABI input, invalid calldata slices, and
// calls to code-less targets. ABI word validators revert with empty data in every mode,
// as in solc. A calldata struct's member offsets are validated lazily on access, so an
// unused malformed member decodes fine, while the same struct decoded to memory reports
// the struct offset. Without a `receive` function, unmatched calls report the "neither
// fallback nor receive" message; see `receive.sol` for the other one.
interface Target {
    function ping() external;
    function value() external returns (uint256);
}

contract CompilerReverts {
    function passthrough(uint256 x) external pure returns (uint256) {
        return x;
    }

    function flag(bool b) external pure returns (bool) {
        return b;
    }

    function slice(bytes calldata data, uint256 start, uint256 end) external pure returns (uint256) {
        bytes calldata sliced = data[start:end];
        return sliced.length;
    }

    struct S {
        uint256 a;
        bytes b;
    }

    function arr(uint256[] calldata values) external pure returns (uint256) {
        return values.length;
    }

    function st(S calldata s) external pure returns (uint256) {
        return s.a;
    }

    function stMem(S memory s) external pure returns (uint256) {
        return s.a;
    }

    function blob(bytes memory data) external pure returns (uint256) {
        return data.length;
    }

    function fixedBytes(bytes[2] memory parts) external pure returns (uint256) {
        return parts[0].length + parts[1].length;
    }

    function unusedBlob(bytes memory) external pure returns (uint256) {
        return 1;
    }

    function dec(bytes memory data) external pure returns (uint256) {
        S memory s = abi.decode(data, (S));
        return s.a;
    }

    function echo(uint256[] calldata values) external pure returns (uint256[] calldata) {
        return values;
    }

    function callNoCode() external {
        Target(address(0x1234)).ping();
    }

    function callNoCodeReturning() external returns (uint256) {
        return Target(address(0x1234)).value();
    }

    function userRevert() external pure {
        revert("user");
    }

    function userRequire(uint256 x) external pure returns (uint256) {
        require(x != 0, "x must be nonzero");
        return x;
    }
}
