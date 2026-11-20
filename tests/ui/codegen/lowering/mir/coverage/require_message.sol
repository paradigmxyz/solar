//@revisions: default strip
//@compile-flags: -O none -Zinstrument-coverage -Zdump=mir
//@[strip] compile-flags: --revert-strings strip
//@[default] filecheck: --enable-var-scope
//@[strip] filecheck: --enable-var-scope --check-prefix=STRIP

// A `require` counts its condition when it holds before its message runs when the message may
// revert while it is evaluated, which is no failure of the condition. A message that cannot, a
// constant, a variable, or a custom error with such arguments, leaves the counter after the check,
// so the code after the `require` stays in one block.
error E(uint256);

string constant MESSAGE = "no";
uint8 constant OVERFLOW = uint8(255) + 1;

struct Item {
    string reason;
}

error Bad(Item);

contract RequireMessage {
    // CHECK-LABEL: fn @f(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 1
    // CHECK-NEXT: coverage 2
    // CHECK-NEXT: jumpi arg1, [[HOLDS:bb[0-9]+]],
    // CHECK: [[HOLDS]]:
    // CHECK-NEXT: coverage 3
    // The message and its bounds check come after.
    // CHECK: icall panic_if<0x32>
    // CHECK: icall require<error_string>
    function f(bytes memory a, bool ok) external pure {
        require(ok, string(abi.encodePacked(a[5])));
    }

    // CHECK-LABEL: fn @named(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: icall require<short_string>, arg0
    // CHECK-NEXT: coverage {{[0-9]+}}
    function named(bool ok) external pure {
        require(ok, MESSAGE);
    }

    // CHECK-LABEL: fn @custom(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: icall require<custom_error [word]>, arg0
    // CHECK-NEXT: coverage {{[0-9]+}}
    function custom(bool ok, uint256 x) external pure {
        require(ok, E(x));
    }

    // An index reads out of bounds, so the counter comes before the message.
    // CHECK-LABEL: fn @field(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: jumpi arg2, [[HELD:bb[0-9]+]],
    // CHECK: [[HELD]]:
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK: icall panic_if<0x32>
    function field(Item[] memory items, uint256 i, bool ok) external pure {
        require(ok, items[i].reason);
    }

    // A field of a variable cannot revert.
    // CHECK-LABEL: fn @plain(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NOT: jumpi
    // CHECK: icall require<error_string>
    function plain(Item memory item, bool ok) external pure {
        require(ok, item.reason);
    }

    // A calldata field checks its offset when it is read, so the counter comes before it.
    // CHECK-LABEL: fn @calldataField(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: jumpi arg{{[0-9]+}}, [[HELD:bb[0-9]+]],
    // CHECK: [[HELD]]:
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK: icall require<error_string>
    function calldataField(Item calldata item, bool ok) external pure {
        require(ok, item.reason);
    }

    // Encoding a calldata struct checks the offsets inside it, so the counter comes before it.
    // CHECK-LABEL: fn @calldataError(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: jumpi arg{{[0-9]+}}, [[HELD:bb[0-9]+]],
    // CHECK: [[HELD]]:
    // CHECK-NEXT: coverage {{[0-9]+}}
    function calldataError(Item calldata item, bool ok) external pure {
        require(ok, Bad(item));
    }

    // A constant that does not fold panics when it is read, so the counter comes before it.
    // CHECK-LABEL: fn @overflow(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: jumpi arg0, [[HELD:bb[0-9]+]],
    function overflow(bool ok) external pure {
        require(ok, E(OVERFLOW));
    }

    function message() internal pure returns (string memory) {
        return "x";
    }

    // The condition counts once when it holds, also when the stripped message keeps only its call.
    // CHECK-LABEL: fn @call(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: jumpi arg0, [[HELD:bb[0-9]+]],
    // CHECK: [[HELD]]:
    // CHECK-NEXT: coverage [[TAKEN:[0-9]+]]
    // CHECK-NOT: coverage [[TAKEN]]{{$}}
    // CHECK: icall require<error_string>
    // CHECK-NOT: coverage [[TAKEN]]{{$}}
    // CHECK: ret
    // STRIP-LABEL: fn @call(
    // STRIP-NEXT: bb0:
    // STRIP-NEXT: coverage {{[0-9]+}}
    // STRIP-NEXT: coverage {{[0-9]+}}
    // STRIP-NEXT: jumpi arg0, [[HELD:bb[0-9]+]],
    // STRIP: [[HELD]]:
    // STRIP-NEXT: coverage [[TAKEN:[0-9]+]]
    // STRIP-NOT: coverage [[TAKEN]]{{$}}
    // STRIP: icall revert_if
    // STRIP-NOT: coverage [[TAKEN]]{{$}}
    // STRIP: ret
    function call(bool ok) external pure {
        require(ok, message());
    }
}
