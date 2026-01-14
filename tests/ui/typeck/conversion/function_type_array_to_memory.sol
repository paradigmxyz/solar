//@compile-flags: -Ztypeck
contract C {
    function externalDefault() external returns(uint) { return 11; }
    function externalView() external view returns(uint) { return 12; }
    function externalPure() external pure returns(uint) { return 13; }

    function internalDefault() internal returns(uint) { return 21; }
    function internalView() internal view returns(uint) { return 22; }
    function internalPure() internal pure returns(uint) { return 23; }

    function testViewToDefault() public returns (uint, uint) {
        function () external returns(uint)[1] memory externalDefaultArray;
        function () internal returns(uint)[1] memory internalDefaultArray;

        // Solar can't infer array element types here
        externalDefaultArray = [this.externalView]; //~ ERROR: cannot infer array element type
        internalDefaultArray = [internalView]; //~ ERROR: cannot infer array element type

        return (externalDefaultArray[0](), internalDefaultArray[0]()); //~ ERROR: not yet implemented
        //~^ ERROR: not yet implemented
    }

    function testPureToDefault() public returns (uint, uint) {
        function () external returns(uint)[1] memory externalDefaultArray;
        function () internal returns(uint)[1] memory internalDefaultArray;

        // Solar can't infer array element types here
        externalDefaultArray = [this.externalPure]; //~ ERROR: cannot infer array element type
        internalDefaultArray = [internalPure]; //~ ERROR: cannot infer array element type

        return (externalDefaultArray[0](), internalDefaultArray[0]()); //~ ERROR: not yet implemented
        //~^ ERROR: not yet implemented
    }

    function testPureToView() public returns (uint, uint) {
        function () external returns(uint)[1] memory externalViewArray;
        function () internal returns(uint)[1] memory internalViewArray;

        // Solar can't infer array element types here
        externalViewArray = [this.externalPure]; //~ ERROR: cannot infer array element type
        internalViewArray = [internalPure]; //~ ERROR: cannot infer array element type

        return (externalViewArray[0](), internalViewArray[0]()); //~ ERROR: not yet implemented
        //~^ ERROR: not yet implemented
    }
}
