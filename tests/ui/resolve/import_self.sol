import "./import_self.sol" as self1;
import "./import_self.sol" as self2;

struct S {
    uint x;
}

contract C {
    function f() external {
        self1.self2.self2.self1.self2.S memory s;
    }
}
