//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract InternalVoidCall {
    // CHECK-LABEL: fn @value{{[( ]}}
    // CHECK: sload 0
    uint256 public value;

    // CHECK-LABEL: fn @set{{[( ]}}
    // CHECK: icall @writeIfNonZero, arg0
    function set(uint256 newValue) public {
        writeIfNonZero(newValue);
    }

    // CHECK-LABEL: fn @writeIfNonZero{{[( ]}}
    // CHECK: [[NONZERO:v[0-9]+]] = ne arg0, 0
    // CHECK: jumpi [[NONZERO]],
    // CHECK: sstore 0, arg0
    function writeIfNonZero(uint256 newValue) internal {
        if (newValue != 0) {
            value = newValue;
        }
    }

    // CHECK-LABEL: fn @setUnlessZero{{[( ]}}
    // CHECK: [[ZERO:v[0-9]+]] = eq arg0, 0
    // CHECK: jumpi [[ZERO]],
    // CHECK: sstore 0, arg0
    function setUnlessZero(uint256 newValue) public {
        if (newValue == 0) {
            return;
        }
        value = newValue;
    }

    // CHECK-LABEL: fn @returnVoidCall{{[( ]}}
    // CHECK: icall @writeIfNonZero, arg0
    // CHECK: ret
    function returnVoidCall(uint256 newValue) public {
        return writeIfNonZero(newValue);
    }

    // CHECK-LABEL: fn @returnRevert{{[( ]}}
    // CHECK: icall revert_if<empty>, true
    function returnRevert() public pure {
        return revert();
    }

    // CHECK-LABEL: fn @unitTernary{{[( ]}}
    // CHECK: jumpi {{(arg[0-9]+|v[0-9]+)}},
    // CHECK: icall @clear
    // CHECK: icall @writeIfNonZero, arg1
    function unitTernary(bool writeValue, uint256 newValue) public {
        writeValue ? writeIfNonZero(newValue) : clear();
    }

    function clear() internal {
        value = 0;
    }
}
