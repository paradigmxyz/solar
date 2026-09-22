//@ codegen-matrix: standard
//@ run-call: Derived::check 0x0000000000000000000000000000000000000000 => false
//@ run-call: Derived::check 0x00000000000000000000000000000000deadbeef => true
//@ run-call: Derived::freeExternal => 3
//@ run-call: Derived::inherited => 30
//@ run-call: Derived::callExternal => 4
//@ run-call: Derived::ownPrivate => true
//@ run-call: Indirect::check 0x0000000000000000000000000000000000000000 => false
//@ run-call: Indirect::freeExternal => 3
//@ run-call: Indirect::inherited => 30
//@ run-call: Indirect::callExternal => 4

import {authorized, externalHelper, internalHelper, publicHelper} from "./auxiliary/inherited_function_visibility.sol";

contract Base {
    function authorized(address) private pure returns (bool) { return true; }
    function externalHelper() external pure returns (uint256) { return 4; }
    function internalHelper() internal pure returns (uint256) { return 10; }
    function publicHelper() public pure returns (uint256) { return 20; }

    function ownPrivate() public pure returns (bool) { return authorized(address(0)); }
}

contract Derived is Base {
    function check(address caller) public pure returns (bool) { return authorized(caller); }
    function freeExternal() public pure returns (uint256) { return externalHelper(); }
    function inherited() public pure returns (uint256) { return internalHelper() + publicHelper(); }
    function callExternal() public view returns (uint256) { return this.externalHelper(); }
}

contract Indirect is Derived {}
