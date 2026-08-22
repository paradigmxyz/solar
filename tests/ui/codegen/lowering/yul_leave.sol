//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: simple() => 2
//@[none, gas, size] run-call: conditional(uint256) 0 => 9
//@[none, gas, size] run-call: conditional(uint256) 1 => 2
//@[none, gas, size] run-call: multiple() => 2, 3
//@[none, gas, size] run-call: fallthrough() => 9

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
