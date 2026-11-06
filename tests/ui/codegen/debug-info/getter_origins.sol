//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A public getter maps to its state variable's declaration, as in solc's `LocationSetter` for the
// variable. The range ends with the name, or with the initializer when there is one, before the
// semicolon: `uint256 public a` is 16 bytes and `uint256 public b = 5` is 20.
// A mapping getter's index and return code take the same range.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: [entry, invoke=a@[[#A:]]
// CHECK-SAME: ..[[#A + 16]]]
// CHECK: return !metadata(span=[[#A]]..[[#A + 16]], exit=return)
// CHECK: [entry, invoke=b@[[#B:]]
// CHECK-SAME: ..[[#B + 20]]]
// CHECK: return !metadata(span=[[#B]]..[[#B + 20]], exit=return)
// CHECK: [entry, invoke=m@[[#M:]]
// CHECK-SAME: ..[[#M + 36]]]
// CHECK: [[#M]]..[[#M + 36]]
contract C {
    uint256 public a;
    uint256 public b = 5;
    mapping(uint256 => uint256) public m;
}
