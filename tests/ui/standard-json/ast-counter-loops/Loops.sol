contract Loops {
    uint256 counter;

    function simple(uint256 n) public pure {
        for (uint256 i = 0; i < n; i++) {}
    }

    function prefix(uint256 n) public pure {
        for (uint256 i = 0; i < n; ++i) {}
    }

    function bodyWrite(uint256 n) public pure {
        for (uint256 i = 0; i < n; i++) {
            i += 2;
        }
    }

    function boundWrite(uint256 n) public pure {
        for (uint256 i = 0; i < n; i++) {
            n = n + 1;
        }
    }

    function tupleWrite(uint256 n) public pure {
        uint256 j;
        for (uint256 i = 0; i < n; i++) {
            (i, j) = (j, i);
        }
    }

    function nestedWrite(uint256 n) public pure {
        for (uint256 i = 0; i < n; i++) {
            for (uint256 k = 0; k < n; k++) {
                i = k;
            }
        }
    }

    function deleted(uint256 n) public pure {
        for (uint256 i = 0; i < n; i++) {
            delete i;
        }
    }

    function stateCounter() public {
        for (counter = 0; counter < 3; counter++) {}
    }

    function decrement(uint256 n) public pure {
        for (uint256 i = n; i < n; i--) {}
    }

    function compare(uint256 n) public pure {
        for (uint256 i = 0; i <= n; i++) {}
    }

    function assemblyWrite() public pure {
        for (uint256 i = 0; i < 42; ++i) {
            assembly {
                i := add(i, 1)
            }
        }
    }

    function assemblyRead(uint256 n) public pure returns (uint256 sum) {
        for (uint256 i = 0; i < n; ++i) {
            assembly {
                sum := add(sum, i)
            }
        }
    }
}
