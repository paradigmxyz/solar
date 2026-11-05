// ported-from: test/libsolidity/syntaxTests/fallback/view_modifier.sol
// ported-from: test/libsolidity/syntaxTests/fallback/pure_modifier.sol

// `view` and `pure` fallback functions are rejected in every kind of contract, whatever the
// signature. The signature is checked independently.

contract ViewBody {
    uint x;
    fallback() external view { x = 2; } //~ ERROR: fallback function must be payable or non-payable
}

contract PureBody {
    uint x;
    fallback() external pure { x = 2; } //~ ERROR: fallback function must be payable or non-payable
}

contract ContractView {
    fallback() external view {} //~ ERROR: fallback function must be payable or non-payable
}

contract ContractViewBytes {
    fallback(bytes calldata) external view returns (bytes memory) {} //~ ERROR: fallback function must be payable or non-payable
}

contract ContractViewParams {
    fallback(uint256) external view {}
    //~^ ERROR: fallback function must be payable or non-payable
    //~| ERROR: invalid fallback function signature
}

contract ContractViewReturns {
    fallback() external view returns (uint256) {}
    //~^ ERROR: fallback function must be payable or non-payable
    //~| ERROR: invalid fallback function signature
}

contract ContractPure {
    fallback() external pure {} //~ ERROR: fallback function must be payable or non-payable
}

contract ContractPureBytes {
    fallback(bytes calldata) external pure returns (bytes memory) {} //~ ERROR: fallback function must be payable or non-payable
}

contract ContractPureParams {
    fallback(uint256) external pure {}
    //~^ ERROR: fallback function must be payable or non-payable
    //~| ERROR: invalid fallback function signature
}

contract ContractPureReturns {
    fallback() external pure returns (uint256) {}
    //~^ ERROR: fallback function must be payable or non-payable
    //~| ERROR: invalid fallback function signature
}

abstract contract AbstractContractView {
    fallback() external view {} //~ ERROR: fallback function must be payable or non-payable
}

abstract contract AbstractContractViewBytes {
    fallback(bytes calldata) external view returns (bytes memory) {} //~ ERROR: fallback function must be payable or non-payable
}

abstract contract AbstractContractViewParams {
    fallback(uint256) external view {}
    //~^ ERROR: fallback function must be payable or non-payable
    //~| ERROR: invalid fallback function signature
}

abstract contract AbstractContractViewReturns {
    fallback() external view returns (uint256) {}
    //~^ ERROR: fallback function must be payable or non-payable
    //~| ERROR: invalid fallback function signature
}

abstract contract AbstractContractPure {
    fallback() external pure {} //~ ERROR: fallback function must be payable or non-payable
}

abstract contract AbstractContractPureBytes {
    fallback(bytes calldata) external pure returns (bytes memory) {} //~ ERROR: fallback function must be payable or non-payable
}

abstract contract AbstractContractPureParams {
    fallback(uint256) external pure {}
    //~^ ERROR: fallback function must be payable or non-payable
    //~| ERROR: invalid fallback function signature
}

abstract contract AbstractContractPureReturns {
    fallback() external pure returns (uint256) {}
    //~^ ERROR: fallback function must be payable or non-payable
    //~| ERROR: invalid fallback function signature
}

interface InterfaceView {
    fallback() external view; //~ ERROR: fallback function must be payable or non-payable
}

interface InterfaceViewBytes {
    fallback(bytes calldata) external view returns (bytes memory); //~ ERROR: fallback function must be payable or non-payable
}

interface InterfaceViewParams {
    fallback(uint256) external view;
    //~^ ERROR: fallback function must be payable or non-payable
    //~| ERROR: invalid fallback function signature
}

interface InterfaceViewReturns {
    fallback() external view returns (uint256);
    //~^ ERROR: fallback function must be payable or non-payable
    //~| ERROR: invalid fallback function signature
}

interface InterfacePure {
    fallback() external pure; //~ ERROR: fallback function must be payable or non-payable
}

interface InterfacePureBytes {
    fallback(bytes calldata) external pure returns (bytes memory); //~ ERROR: fallback function must be payable or non-payable
}

interface InterfacePureParams {
    fallback(uint256) external pure;
    //~^ ERROR: fallback function must be payable or non-payable
    //~| ERROR: invalid fallback function signature
}

interface InterfacePureReturns {
    fallback() external pure returns (uint256);
    //~^ ERROR: fallback function must be payable or non-payable
    //~| ERROR: invalid fallback function signature
}

// Payable and non-payable fallback functions are accepted.

contract ContractPayable {
    fallback() external payable {}
}

contract ContractPayableBytes {
    fallback(bytes calldata) external payable returns (bytes memory) {}
}

contract ContractNonPayable {
    fallback() external {}
}

contract ContractNonPayableBytes {
    fallback(bytes calldata) external returns (bytes memory) {}
}

abstract contract AbstractContractPayable {
    fallback() external payable {}
}

abstract contract AbstractContractPayableBytes {
    fallback(bytes calldata) external payable returns (bytes memory) {}
}

abstract contract AbstractContractNonPayable {
    fallback() external {}
}

abstract contract AbstractContractNonPayableBytes {
    fallback(bytes calldata) external returns (bytes memory) {}
}

interface InterfacePayable {
    fallback() external payable;
}

interface InterfacePayableBytes {
    fallback(bytes calldata) external payable returns (bytes memory);
}

interface InterfaceNonPayable {
    fallback() external;
}

interface InterfaceNonPayableBytes {
    fallback(bytes calldata) external returns (bytes memory);
}
