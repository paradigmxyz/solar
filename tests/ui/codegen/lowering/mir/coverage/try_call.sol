//@compile-flags: -O none -Zinstrument-coverage -Zdump=mir
//@filecheck: --enable-var-scope

// A `try` counts the calls that returned, right after the call, and each clause counts against
// them, so a revert while the arguments are evaluated or before the call runs is no failure of a
// clause. The success clause holds when the call returned, so a revert while its return data is
// decoded, such as a `uint8` above 255, counts it as taken: its counter comes before the decode.
contract Callee {
    function g(uint256 a) external pure returns (uint256) {
        return a;
    }
}

interface Narrow {
    function g() external pure returns (uint8);
}

contract TryCall {
    // CHECK-LABEL: fn @f(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 2
    // CHECK: staticcall
    // CHECK-NEXT: coverage 3
    // CHECK-NEXT: jumpi
    function f(Callee c) external pure returns (bool ok) {
        try c.g(1) returns (uint256) {
            ok = true;
        } catch {
            ok = false;
        }
    }

    // CHECK-LABEL: fn @decode(
    // CHECK: staticcall
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: jumpi
    // CHECK: bb{{[0-9]+}}:
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: v{{[0-9]+}} = returndatasize
    // CHECK: icall revert_if<tuple_data_too_short>
    // CHECK: icall revert_if<empty>
    // CHECK-NEXT: coverage {{[0-9]+}}
    function decode(Narrow c) external pure returns (uint8 r) {
        try c.g() returns (uint8 v) {
            r = v;
        } catch {}
    }
}
