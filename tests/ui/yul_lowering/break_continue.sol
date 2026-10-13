contract C {
    function ok(uint256 n) public returns (uint256 x) {
        assembly {
            for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                if eq(i, 1) {
                    continue
                }
                if eq(i, 3) {
                    break
                }
                x := add(x, i)
            }
        }
    }

    function outside() public {
        assembly {
            break //~ ERROR: keyword `break` needs to be inside a for-loop body
            continue //~ ERROR: keyword `continue` needs to be inside a for-loop body
        }
    }

    function post_block() public {
        assembly {
            for {} 1 {
                break //~ ERROR: keyword `break` in for-loop post block is not allowed
                continue //~ ERROR: keyword `continue` in for-loop post block is not allowed
            } {}
        }
    }

    function nested_loop_in_init_and_post() public {
        assembly {
            for {
                for {} 1 {} {
                    break
                    continue
                }
            } 0 {
                for {} 1 {} {
                    break
                    continue
                }
            } {}
        }
    }

    function nested_function(uint256 n) public {
        assembly {
            for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                function bad() {
                    break //~ ERROR: keyword `break` needs to be inside a for-loop body
                    continue //~ ERROR: keyword `continue` needs to be inside a for-loop body
                }
            }
        }
    }
}
