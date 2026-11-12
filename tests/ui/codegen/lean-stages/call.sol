//@ compile-flags: --emit=bin-runtime --evm-version osaka -Zemit-lean-stages=../../target/ui-lean-stages
// External calls take their success flag, their return data and the storage after them from
// the semantics' oracle, and `returndatacopy` copies that return data, so their stages export.

interface IPrice {
    function price() external view returns (uint256);
}

contract LeanStagesCall {
    function f(address a) external returns (bool) {
        (bool ok,) = a.call{gas: 5000, value: 1}("");
        return ok;
    }

    function g(IPrice p) external view returns (uint256) {
        return p.price() + 1;
    }

    function h(address a, bytes calldata data) external returns (bytes memory) {
        (bool ok, bytes memory out) = a.delegatecall(data);
        require(ok);
        return out;
    }
}
