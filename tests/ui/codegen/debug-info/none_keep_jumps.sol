//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// An unoptimized build keeps the jump of each `break` and `continue` statement and the tail call
// of a call statement, including the one of a function that only forwards, each as an instruction
// with the origin of its own statement, so that coverage can anchor it.

// CHECK-LABEL: (runtime) ===
// CHECK: [entry, invoke=loop@
// CHECK: push bb{{[0-9]+}} !metadata(span=1274..1280, keep_with_next)
// CHECK-NEXT: jump !metadata(span=1274..1280)
// CHECK: push bb{{[0-9]+}} !metadata(span=1208..1217, keep_with_next)
// CHECK-NEXT: jump !metadata(span=1208..1217)
// CHECK: [entry, invoke=set@
// CHECK: push bb{{[0-9]+}} !metadata(span=1573..1580, keep_with_next)
// CHECK-NEXT: jump !metadata(span=1573..1580, exit=return)
// CHECK: [invoke=_forward@
// CHECK-NEXT: push bb{{[0-9]+}} !metadata(span=1737..1746, keep_with_next)
// CHECK-NEXT: jump !metadata(span=1737..1746, exit=return)
contract C {
    uint256 public total;

    function loop(uint256 n) external pure returns (uint256 sum) {
        for (uint256 i; i < n; i++) {
            if (i == 3) {
                continue;
            }
            if (i == 7) {
                break;
            }
            sum += i;
        }
    }

    function set(uint256 x) external {
        _init(x);
    }

    function forward(uint256 x) external pure returns (uint256) {
        return _forward(x);
    }

    function _init(uint256 x) internal {
        require(x != 0);
        _set(x);
    }

    function _set(uint256 x) internal {
        total = x;
    }

    function _forward(uint256 x) internal pure returns (uint256) {
        return _twice(x);
    }

    function _twice(uint256 x) internal pure returns (uint256) {
        return x * 2;
    }
}
