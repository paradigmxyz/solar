//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: simple() => 2
//@ run-call: conditional(uint256) 0 => 9
//@ run-call: conditional(uint256) 1 => 2
//@ run-call: multiple() => 2, 3
//@ run-call: fallthrough() => 9

contract YulLeave {
    function simple() external pure returns (uint256 result) {
        assembly {
            function value() -> output {
                output := 2
                leave
                output := 9
            }
            result := value()
        }
    }

    function conditional(uint256 condition) external pure returns (uint256 result) {
        assembly {
            function value(flag) -> output {
                output := 9
                if flag {
                    output := 2
                    leave
                }
            }
            result := value(condition)
        }
    }

    function multiple() external pure returns (uint256 first, uint256 second) {
        assembly {
            function values() -> a, b {
                a := 2
                b := 3
                leave
                a := 8
                b := 9
            }
            first, second := values()
        }
    }

    function fallthrough() external pure returns (uint256 result) {
        assembly {
            function value() -> output {
                output := 9
            }
            result := value()
        }
    }
}
