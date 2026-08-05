//@ run-call: f(bool) false => true
//@ run-call: f(bool) true => false

contract TryCreationChild {
    constructor(bool fail) {
        require(!fail, "x");
    }
}

contract TryCreation {
    function f(bool fail) external returns (bool) {
        try new TryCreationChild(fail) returns (TryCreationChild child) {
            return address(child) != address(0);
        } catch {
            return false;
        }
    }
}
