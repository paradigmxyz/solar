import "./import_self.sol" as self1;
import "./import_self.sol" as self2;
import { S, S as S2 } from "./import_self.sol";

struct S {
    uint x;
}

contract C {
    function f() external {
        self1.self2.self2.self1.self2.S memory s;
        S2 memory s2;
    }
}
