//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call-fail: invalidShort => Panic(0x22)
//@ run-call-fail: invalidLong => Panic(0x22)
//@ run-call-fail: invalidShortDelete => Panic(0x22)
//@ run-call-fail: invalidLongDelete => Panic(0x22)
//@ run-call-fail: invalidShortLength => Panic(0x22)
//@ run-call-fail: invalidLongLength => Panic(0x22)
//@ run-call-fail: invalidShortIndex => Panic(0x22)
//@ run-call-fail: invalidLongIndex => Panic(0x22)
//@ run-call-fail: invalidShortIndexWrite => Panic(0x22)
//@ run-call-fail: invalidLongIndexWrite => Panic(0x22)
//@ run-call-fail: invalidShortIndexDelete => Panic(0x22)
//@ run-call-fail: invalidLongIndexDelete => Panic(0x22)
//@ run-call-fail: encodingCheckedBeforeBounds => Panic(0x22)
//@ run-call-fail: encodingCheckedBeforeBoundsWrite => Panic(0x22)
//@ run-call-fail: invalidShortPush => Panic(0x22)
//@ run-call-fail: invalidLongPush => Panic(0x22)
//@ run-call-fail: invalidShortPushZero => Panic(0x22)
//@ run-call-fail: invalidLongPushZero => Panic(0x22)
//@ run-call-fail: invalidShortPushAssign => Panic(0x22)
//@ run-call-fail: invalidLongPushAssign => Panic(0x22)
//@ run-call-fail: invalidShortPop => Panic(0x22)
//@ run-call-fail: invalidLongPop => Panic(0x22)
//@ run-call: pushAtMaxLength => 36893488147419103233
//@ run-call-fail: pushPastMaxLength => Panic(0x41)
//@ run-call-fail: pushZeroPastMaxLength => Panic(0x41)

contract StorageBytesValidation {
    bytes private data;

    function invalidShort() external returns (bytes memory) {
        assembly {
            // A short encoding cannot claim 32 bytes.
            sstore(data.slot, 0x40)
        }
        return data;
    }

    function invalidLong() external returns (bytes memory) {
        assembly {
            // A long encoding must contain at least 32 bytes.
            sstore(data.slot, 0x21)
        }
        return data;
    }

    function invalidShortDelete() external {
        assembly {
            sstore(data.slot, 0x40)
        }
        delete data;
    }

    function invalidLongDelete() external {
        assembly {
            sstore(data.slot, 0x21)
        }
        delete data;
    }

    function invalidShortLength() external returns (uint256) {
        assembly {
            sstore(data.slot, 0x40)
        }
        return data.length;
    }

    function invalidLongLength() external returns (uint256) {
        assembly {
            sstore(data.slot, 0x21)
        }
        return data.length;
    }

    function invalidShortIndex() external returns (bytes1) {
        assembly {
            sstore(data.slot, 0x40)
        }
        return data[0];
    }

    function invalidLongIndex() external returns (bytes1) {
        assembly {
            sstore(data.slot, 0x21)
        }
        return data[0];
    }

    function invalidShortIndexWrite() external {
        assembly {
            sstore(data.slot, 0x40)
        }
        data[0] = 0x01;
    }

    function invalidLongIndexWrite() external {
        assembly {
            sstore(data.slot, 0x21)
        }
        data[0] = 0x01;
    }

    function invalidShortIndexDelete() external {
        assembly {
            sstore(data.slot, 0x40)
        }
        delete data[0];
    }

    function invalidLongIndexDelete() external {
        assembly {
            sstore(data.slot, 0x21)
        }
        delete data[0];
    }

    // The encoding check runs before the bounds check, so an index past the
    // decoded length still reports the encoding panic and not `Panic(0x32)`.
    function encodingCheckedBeforeBounds() external returns (bytes1) {
        assembly {
            sstore(data.slot, 0x21)
        }
        return data[100];
    }

    function encodingCheckedBeforeBoundsWrite() external {
        assembly {
            sstore(data.slot, 0x21)
        }
        data[100] = 0x01;
    }

    // `push`, `push()`, and `pop` decode the same header, so an invalid
    // encoding stops them before they touch the value.
    function invalidShortPush() external {
        assembly {
            sstore(data.slot, 0x40)
        }
        data.push(0x01);
    }

    function invalidLongPush() external {
        assembly {
            sstore(data.slot, 0x21)
        }
        data.push(0x01);
    }

    function invalidShortPushZero() external {
        assembly {
            sstore(data.slot, 0x40)
        }
        data.push();
    }

    function invalidLongPushZero() external {
        assembly {
            sstore(data.slot, 0x21)
        }
        data.push();
    }

    function invalidShortPushAssign() external {
        assembly {
            sstore(data.slot, 0x40)
        }
        data.push() = 0x01;
    }

    function invalidLongPushAssign() external {
        assembly {
            sstore(data.slot, 0x21)
        }
        data.push() = 0x01;
    }

    function invalidShortPop() external {
        assembly {
            sstore(data.slot, 0x40)
        }
        data.pop();
    }

    function invalidLongPop() external {
        assembly {
            sstore(data.slot, 0x21)
        }
        data.pop();
    }

    // A push that lands exactly on 2**64 bytes is still allowed; going past it
    // panics, for both push forms.
    function pushAtMaxLength() external returns (uint256 header) {
        assembly {
            sstore(data.slot, add(mul(sub(shl(64, 1), 1), 2), 1))
        }
        data.push(0x01);
        assembly {
            header := sload(data.slot)
        }
    }

    function pushPastMaxLength() external {
        assembly {
            sstore(data.slot, add(mul(shl(64, 1), 2), 1))
        }
        data.push(0x01);
    }

    function pushZeroPastMaxLength() external {
        assembly {
            sstore(data.slot, add(mul(shl(64, 1), 2), 1))
        }
        data.push();
    }
}
