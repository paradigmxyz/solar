contract test {
    modifier e() {
      _;
    }
    function f() external {
      _; //~ ERROR: placeholder statements can only be used in modifiers
    }
}