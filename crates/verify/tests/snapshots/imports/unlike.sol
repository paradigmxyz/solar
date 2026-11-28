pragma solidity ^0.8.20;

// Two declarations of `IERC20` whose `transfer` returns differ cannot stand for one another.
import {IERC20} from "./auxiliary/TokenA.sol";
import {PayC} from "./auxiliary/TokenC.sol";

contract Unlike {}
