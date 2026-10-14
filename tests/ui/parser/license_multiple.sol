// SPDX-License-Identifier: MIT
contract C {
    // Comments inside items are ignored.
    // SPDX-License-Identifier: GPL-3.0
}
/* SPDX-License-Identifier: MIT */
//~^ ERROR: multiple SPDX license identifiers found in source file
