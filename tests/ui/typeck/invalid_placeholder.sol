contract test {
    modifier e() {
      _;
    }
    function f() external {
      _; //~ ERROR: unresolved symbol `_`
    }
}
