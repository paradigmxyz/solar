//@ compile-flags: -Ztypeck

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
            break //~ ERROR: `break` outside of a loop
            continue //~ ERROR: `continue` outside of a loop
        }
    }

    // TODO: solc rejects break/continue in Yul for-loop init/post blocks.
    // Solar only checks whether the lowered HIR statement is inside a loop.
    function post_block_diverges_from_solc() public {
        assembly {
            for {} 1 {
                break
                continue
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
                    break //~ ERROR: `break` outside of a loop
                    continue //~ ERROR: `continue` outside of a loop
                }
            }
        }
    }
}
