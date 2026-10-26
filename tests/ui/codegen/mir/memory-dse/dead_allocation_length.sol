//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: 0xdeadbeef624fbfdc000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000021234000000000000000000000000000000000000000000000000000000000000 => 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000021234000000000000000000000000000000000000000000000000000000000000
//@ run-call: 0xdeadbeef624fbfdc00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000021ababababababababababababababababababababababababababababababababab00000000000000000000000000000000000000000000000000000000000000 => 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000021ababababababababababababababababababababababababababababababababab00000000000000000000000000000000000000000000000000000000000000
//@ run-call: 0xdeadbeef624fbfdc00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000 => 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: 0xdeadbeef132e4f3c0000000000000000000000000000000000000000000000000000000000000007 => 0x0000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: 0xdeadbeef5ed7ca5b => 0x
//@ run-call-fail: 0xdeadbeef
//@ run-call: relay 0x1234 => 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000021234000000000000000000000000000000000000000000000000000000000000
pragma solidity ^0.8.0;

// A `bytes` object handed to a call, and the returndata copy a proxy bubbles up, keep their
// length words in memory only for the code that reads them. Once forwarding replaces those
// reads, the call and the revert read just the data after the word, so the word's store goes.
// The returned and reverted bytes stay exact.
contract DeadAllocationLength {
    function echo(bytes calldata data) external pure returns (bytes memory) {
        return data;
    }

    function fail(uint256 code) external pure {
        assembly {
            mstore(0, code)
            revert(0, 32)
        }
    }

    function halt() external pure {
        revert();
    }

    function relay(bytes calldata data) external returns (bytes memory out) {
        bool ok;
        (ok, out) = address(this).call(abi.encodeWithSelector(this.echo.selector, data));
        require(ok);
    }

    // CHECK-LABEL: fn @fallback.body
    // CHECK-NOT: mstore v{{[0-9]+}},
    // CHECK: delegatecall
    // CHECK-NOT: mstore v{{[0-9]+}},
    // CHECK: revert
    fallback(bytes calldata input) external returns (bytes memory) {
        (bool ok, bytes memory out) = address(this).delegatecall(input[4:]);
        if (!ok) {
            assembly ("memory-safe") {
                revert(add(out, 32), mload(out))
            }
        }
        return out;
    }
}
