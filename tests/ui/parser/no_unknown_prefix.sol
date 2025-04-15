//@compile-flags: --stop-after parsing

// This used to fail with an "unknown string prefix" error.

import"does_not_exist";
import {a}from"does_not_exist";
import {b} from"does_not_exist";
