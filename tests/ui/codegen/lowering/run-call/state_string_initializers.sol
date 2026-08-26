//@ codegen-matrix: standard
//@ run-call: deploys() => true
//@ run-call: deploy() => "NativeERC20", "NERC20", 18

contract StateStringInitializers {
    string public name = "NativeERC20";
    string public symbol = "NERC20";
    uint8 public decimals = 18;
}

contract DeployStateStringInitializers {
    function deploys() external returns (bool) {
        StateStringInitializers token = new StateStringInitializers();
        return address(token).code.length != 0;
    }

    function deploy() external returns (string memory, string memory, uint8) {
        StateStringInitializers token = new StateStringInitializers();
        return (token.name(), token.symbol(), token.decimals());
    }
}
