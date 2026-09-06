//@ codegen-matrix: standard
//@[gas,size] compile-flags: -Zdump=evm-ir-runtime
//@[gas,size] filecheck:
//@ run-call: GasFrontierObserver::s => 0
//@ run-call: CodeFrontierObserver::s => 0
//@ run-call: ForwardedGasFrontierObserver::s => 0
//@[none] run-call: CodeFrontierObserver::observed; gas=100000 => 277
//@[none] run-call: ForwardedGasFrontierObserver::observed 0x0000000000000000000000000000000000001111; gas=100000 => 160
//@[none] run-call: GasFrontierObserver::observed; gas=100000 => 78925
//@[gas] run-call: CodeFrontierObserver::observed; gas=100000 => 221
//@[gas] run-call: ForwardedGasFrontierObserver::observed 0x0000000000000000000000000000000000001111; gas=100000 => 160
//@[gas] run-call: GasFrontierObserver::observed; gas=100000 => 79009
//@[size] run-call: CodeFrontierObserver::observed; gas=100000 => 221
//@[size] run-call: ForwardedGasFrontierObserver::observed 0x0000000000000000000000000000000000001111; gas=100000 => 160
//@[size] run-call: GasFrontierObserver::observed; gas=100000 => 79009

// Keep initialization global when code, gas, or forwarded gas can be observed.
// GAS and forwarded calls are explicit barriers: retained ModRef does not mark calls as GAS.
// CHECK-LABEL: @module GasFrontierObserver_runtime
// CHECK: push 64
// CHECK-NEXT: mstore
// CHECK: push 224
// CHECK-NEXT: shr
contract GasFrontierObserver {
    uint256 public s;
    function observed() external view returns (uint256 result) {
        assembly { result := add(mload(64), gas()) }
    }
}
// CHECK-LABEL: @module CodeFrontierObserver_runtime
// CHECK: push 64
// CHECK-NEXT: mstore
// CHECK: push 224
// CHECK-NEXT: shr
contract CodeFrontierObserver {
    uint256 public s;
    function observed() external pure returns (uint256 result) {
        assembly { result := add(mload(64), codesize()) }
    }
}
// CHECK-LABEL: @module ForwardedGasFrontierObserver_runtime
// CHECK: push 64
// CHECK-NEXT: mstore
// CHECK: push 224
// CHECK-NEXT: shr
contract ForwardedGasFrontierObserver {
    uint256 public s;
    function observed(address target) external view returns (uint256 result) {
        assembly {
            result := mload(64)
            pop(staticcall(gas(), target, 0, 0, 0, 0))
        }
    }
}
