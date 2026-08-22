//@ run-call: value() => 1
//@ run-call: valueWithArg() => 2
//@ run-call: valueWithTwo() => 2
//@ run-call: valueWithTwoPlaceholders() => 2
//@ run-call: repeatFalse() => 1
//@ run-call: repeatTrue() => 2
//@ run-call: modifierMutates 1 => 1

contract ModifierExpansion {
    uint256 private count;

    modifier around() {
        count += 1;
        _;
        count += 2;
    }

    modifier add(uint256 amount) {
        count += amount;
        _;
        count += amount;
    }

    modifier twice() {
        _;
        _;
    }

    modifier repeat(bool twice) {
        if (twice) _;
        _;
    }

    modifier mutates(uint256 amount) {
        amount = 7;
        _;
    }

    function value() external around returns (uint256) {
        return count;
    }

    function valueWithArg() external add(2) returns (uint256) {
        return count;
    }

    function valueWithTwo() external around around returns (uint256) {
        return count;
    }

    function valueWithTwoPlaceholders() external twice returns (uint256) {
        count += 1;
        return count;
    }

    function repeatFalse() external repeat(false) returns (uint256 result) {
        result += count + 1;
        count = result;
        return result;
    }

    function repeatTrue() external repeat(true) returns (uint256 result) {
        result += count + 1;
        count = result;
        return result;
    }

    function modifierMutates(uint256 value) external mutates(2) returns (uint256) {
        //~^ WARN: function state mutability can be restricted to pure
        return value;
    }
}
