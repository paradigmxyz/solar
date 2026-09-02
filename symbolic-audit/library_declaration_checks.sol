library L {
    constructor() {}
    fallback() external {}
    function v() public virtual {}
}
contract C {
    function p() internal payable {}
    function q() private payable {}
}
function free() payable {}
