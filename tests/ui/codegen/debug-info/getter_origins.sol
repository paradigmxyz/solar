//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A public getter maps to its state variable's declaration, as in solc's `LocationSetter` for the
// variable. The range ends with the name, or with the initializer when there is one, before the
// semicolon: `uint256 public a` is 16 bytes and `uint256 public b = 5` is 20.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: [entry, invoke=a@[[#A:]]..{{[0-9]+}}]
// CHECK: return !metadata(span=[[#A]]..[[#A + 16]], exit=return)
// CHECK: [entry, invoke=b@[[#B:]]..{{[0-9]+}}]
// CHECK: return !metadata(span=[[#B]]..[[#B + 20]], exit=return)
contract C {
    uint256 public a;
    uint256 public b = 5;
}
