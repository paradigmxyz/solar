//@compile-flags: -O none -Zinstrument-coverage -Zdump=mir
//@filecheck: --enable-var-scope

// `@custom:solar-coverage off` leaves code out of coverage: it gets no probes. The innermost tag
// wins, so `on` puts a function back in a contract that is off, and a modifier that is off has no
// probes in the functions that invoke it.
contract Tags {
    uint256 public total;

    /// @custom:solar-coverage off
    modifier quiet(uint256 a) {
        require(a != 0);
        _;
    }

    modifier loud(uint256 a) {
        require(a != 0);
        _;
    }

    // Neither the function nor the expansion of the modifier it invokes counts.
    // CHECK-LABEL: fn @hidden(
    // CHECK-NOT: coverage {{[0-9]}}
    // CHECK: icall revert_if<empty>
    // CHECK-NOT: coverage {{[0-9]}}
    // CHECK: sstore
    /// @custom:solar-coverage off
    function hidden(uint256 a) external loud(a) {
        total = a;
    }

    // The function counts its entry and its body, and the body of `quiet` has no probes.
    // CHECK-LABEL: fn @shown(
    // CHECK: coverage {{[0-9]}}
    // CHECK: icall revert_if<empty>
    // CHECK: coverage {{[0-9]}}
    // CHECK: sstore
    // CHECK-NOT: coverage {{[0-9]}}
    // CHECK-NOT: icall revert_if
    // CHECK-LABEL: @module Off
    function shown(uint256 a) external quiet(a) {
        total = a;
    }
}

/// @custom:solar-coverage off
contract Off {
    uint256 public total;

    // CHECK-LABEL: fn @inherited(
    // CHECK-NOT: coverage {{[0-9]}}
    // CHECK: sstore
    function inherited(uint256 a) external {
        if (a > 1) {
            total = a;
        }
    }

    // CHECK-LABEL: fn @restored(
    // CHECK: coverage {{[0-9]}}
    // CHECK: sstore
    /// @custom:solar-coverage on
    function restored(uint256 a) external {
        if (a > 1) {
            total = a;
        }
    }
}
