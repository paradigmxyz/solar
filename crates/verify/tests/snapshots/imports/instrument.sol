pragma solidity ^0.8.20;

import "./auxiliary/Checked.sol";

/// Verification assumes the imported check, which `--instrument` cannot write into this file.
contract Instrumented is Checked {}
