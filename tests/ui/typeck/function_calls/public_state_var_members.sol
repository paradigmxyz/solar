//@compile-flags: -Ztypeck

contract PublicStateVarBase {
    uint256 public baseVar = 42;
}

contract PublicStateVarDerived is PublicStateVarBase {
    uint256 public selfVar = 24;

    function getStateVars() public view returns (uint256 a, uint256 b, uint256 c, uint256 d) {
        a = PublicStateVarDerived.selfVar;
        b = PublicStateVarBase.baseVar;
        c = this.selfVar();
        d = this.baseVar();

        this.selfVar;
        this.baseVar;

        PublicStateVarDerived.selfVar(); //~ ERROR: expected function, found `uint256`
        PublicStateVarBase.baseVar(); //~ ERROR: expected function, found `uint256`
        super.baseVar; //~ ERROR: member `baseVar` not found
        super.baseVar(); //~ ERROR: member `baseVar` not found
    }
}

contract PublicStateVarOtherScope {
    function getStateVars() public view {
        PublicStateVarDerived.selfVar;
        PublicStateVarDerived.baseVar;
        PublicStateVarBase.baseVar;

        PublicStateVarDerived.selfVar(); //~ ERROR: cannot call function via contract type name
        PublicStateVarDerived.baseVar(); //~ ERROR: cannot call function via contract type name
        PublicStateVarBase.baseVar(); //~ ERROR: cannot call function via contract type name
    }
}
