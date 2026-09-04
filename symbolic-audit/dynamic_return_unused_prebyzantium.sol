// Finding 39: before byzantium a call whose dynamically encoded return value is not used is
// legal in solc (the value has an inaccessible dynamic type); we reject it while trying to
// decode the return data.
//   solc --bin --via-ir --evm-version homestead symbolic-audit/dynamic_return_unused_prebyzantium.sol
//   target/debug/solar --evm-version homestead --emit bin symbolic-audit/dynamic_return_unused_prebyzantium.sol
interface I { function dyn() external returns (bytes memory); }
contract C {
    function t(address a) external { I(a).dyn(); }
    function u(address a) external { try I(a).dyn() { } catch { } }
}
