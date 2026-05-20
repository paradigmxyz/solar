contract CallOptionMembers {
    function g() external {}
    function h() external payable {}

    function nested() external {
        this.h{gas: 42}{value: 5}(); //~ ERROR: function call options have already been set
    }

    function callOptionMembers() external returns (bool) {
        return this.g{gas: 42}.address == this.g.address && //~ ERROR: call options must be part of a call expression
            this.g{gas: 42}.selector == this.g.selector && //~ ERROR: call options must be part of a call expression
            this.h{gas: 42}.address == this.h.address && //~ ERROR: call options must be part of a call expression
            this.h{gas: 42}.selector == this.h.selector && //~ ERROR: call options must be part of a call expression
            this.h{gas: 42, value: 5}.address == this.h.address && //~ ERROR: call options must be part of a call expression
            this.h{gas: 42, value: 5}.selector == this.h.selector; //~ ERROR: call options must be part of a call expression
    }
}
