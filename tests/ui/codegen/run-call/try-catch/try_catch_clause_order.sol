//@ codegen-matrix: standard
//@ run-call: lowLevelFirst 0 => 42
//@ run-call: lowLevelFirst 1 => 200
//@ run-call: lowLevelFirst 2 => 300
//@ run-call: lowLevelFirst 3 => 100
//@ run-call: typedFirst 0 => 42
//@ run-call: typedFirst 1 => 200
//@ run-call: typedFirst 2 => 300
//@ run-call: typedFirst 3 => 100
//@ run-call: bareFirst 0 => 42
//@ run-call: bareFirst 1 => 100
//@ run-call: bareFirst 2 => 300
//@ run-call: bareFirst 3 => 100
// A `try` statement's typed catch clauses are matched before the low-level
// one, whichever order they are written in. The low-level clause matches
// every revert payload, so testing the clauses in source order let one
// written first shadow `catch Error(string)` and `catch Panic(uint256)`:
// `lowLevelFirst(1)` returned 100 instead of 200 and `lowLevelFirst(2)`
// returned 100 instead of 300. The expected values are solc's.
contract C {
    function boom(uint256 k) external pure returns (uint256) {
        if (k == 1) {
            revert("nope");
        }
        if (k == 2) {
            uint256 z = 0;
            return 1 / z;
        }
        if (k == 3) {
            revert();
        }
        return 42;
    }

    function lowLevelFirst(uint256 k) public view returns (uint256) {
        try this.boom(k) returns (uint256 v) {
            return v;
        } catch (bytes memory) {
            return 100;
        } catch Error(string memory) {
            return 200;
        } catch Panic(uint256) {
            return 300;
        }
    }

    function typedFirst(uint256 k) public view returns (uint256) {
        try this.boom(k) returns (uint256 v) {
            return v;
        } catch Error(string memory) {
            return 200;
        } catch Panic(uint256) {
            return 300;
        } catch (bytes memory) {
            return 100;
        }
    }

    function bareFirst(uint256 k) public view returns (uint256) {
        try this.boom(k) returns (uint256 v) {
            return v;
        } catch {
            return 100;
        } catch Panic(uint256) {
            return 300;
        }
    }
}
