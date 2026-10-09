contract C {
    function f(uint256 n) public {
        for (;;) {
            assembly {
                break //~ ERROR: keyword `break` needs to be inside a for-loop body
                continue //~ ERROR: keyword `continue` needs to be inside a for-loop body
            }
        }
        while (true) {
            assembly {
                if 1 { break } //~ ERROR: keyword `break` needs to be inside a for-loop body
            }
        }
        do {
            assembly {
                { continue } //~ ERROR: keyword `continue` needs to be inside a for-loop body
            }
        } while (true);
        for (uint256 i; i < n; i++) {
            assembly {
                for { break } 1 { continue } { //~ ERROR: keyword `break` in for-loop init block is not allowed
                //~^ ERROR: keyword `continue` in for-loop post block is not allowed
                    function g() {
                        break //~ ERROR: keyword `break` needs to be inside a for-loop body
                    }
                    break
                    continue
                }
            }
        }
    }
}
