contract CallOptionMembers {
    function g() external {}
    function h() external payable {}

    function nested() external {
        this.h{gas: 42}{value: 5}(); //~ ERROR: function call options have already been set
    }

    function callOptionMembers() external returns (bool) {
        return this.g{gas: 42}.address == this.g.address &&
            this.g{gas: 42}.selector == this.g.selector &&
            this.h{gas: 42}.address == this.h.address &&
            this.h{gas: 42}.selector == this.h.selector &&
            this.h{gas: 42, value: 5}.address == this.h.address &&
            this.h{gas: 42, value: 5}.selector == this.h.selector;
    }
    function assign() external view {
        function() external stored = this.g{gas: 1}; //~ ERROR: mismatched types
        this.g{gas: true}; //~ ERROR: mismatched types
    }
}
