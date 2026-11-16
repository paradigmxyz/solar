import "./import_twice.sol" as self;
import "./import_twice.sol" as self;

contract C {}
contract D is self.C {}
