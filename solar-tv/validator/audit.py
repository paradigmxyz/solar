"""Versioned audit records and independent status totals."""

import argparse
import hashlib
import json
import os
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

from .common import Program

ROOT = Path(__file__).resolve().parents[1]
STATUSES = (
    "verified",
    "counterexample",
    "unsupported",
    "timeout",
    "inconclusive",
    "error",
)


@dataclass
class Result:
    status: str
    stdout: str = ""
    stderr: str = ""
    program: Program | None = None
    witness: dict = field(default_factory=dict)
    pair: tuple[Program, Program] | None = None

    @property
    def exit_code(self) -> int:
        return (
            0
            if self.status == "verified"
            else 1
            if self.status == "counterexample"
            else 2
        )


def semantics() -> dict:
    digest = hashlib.sha256()
    paths = [
        *ROOT.glob("SolarTV/**/*.lean"),
        ROOT / "SolarTV.lean",
        *ROOT.glob("validator/*.py"),
        ROOT / "lean-toolchain",
    ]
    for path in sorted(paths):
        digest.update(
            path.relative_to(ROOT).as_posix().encode()
            + b"\0"
            + path.read_bytes()
            + b"\0"
        )
    dependency = ROOT.parent / "scripts/evm-rules/lean"
    for path in sorted(dependency.glob("EvmRules/**/*.lean")):
        digest.update(
            path.relative_to(dependency).as_posix().encode()
            + b"\0"
            + path.read_bytes()
            + b"\0"
        )
    return {
        "version": "solar-tv-v2",
        "sha256": digest.hexdigest(),
        "lean": (ROOT / "lean-toolchain").read_text().strip(),
    }


def record(
    result: Result,
    *,
    layer: str,
    target: str,
    policy: str | None,
    before: Path,
    after: Path,
    model: dict | None = None,
) -> dict:
    program = result.program
    assumptions = [
        "compiler-validated IR",
        "defined inputs",
        "trusted parser and importer",
    ]
    if program and program.world:
        fixture = program.world
        assumptions.extend(
            [
                f"explicit initial world sha256={fixture['fixture_sha256']}; omitted accounts absent and slots zero",
                "explicit block context; omitted history hashes zero; transaction admission/authorization processing excluded",
                f"model work limit {fixture['fuel']}; 64 KiB memory limit; unsupported paths prevent verification",
                "scenario proof; no arbitrary-world or whole-pass claim",
            ]
        )
        if program.scope == "return-path":
            assumptions.extend(
                [
                    "concrete funded entry-to-exit paths start at runtime PC zero and independently execute each physical JUMP",
                    "live stack, memory, returndata and persistent effects are compared; no claim for other branches or equal physical gas",
                ]
            )
            if result.status == "verified":
                assumptions.append(
                    "conditional unpriced path correspondence for shared calldata and live state following the checked guards; the encoded return address is consumed only by its physical return"
                )
        elif fixture.get("direct_mir"):
            assumptions.extend(
                [
                    "direct typed MIR SSA execution; no backend lowering or implicit frame/FMP setup",
                    "listed symbolic storage cells and typed function arguments universally quantified",
                    "explicit initial byte memory; normal function returns expose memory and transient storage",
                    "semantic effects; abstract MIR instructions unpriced; physical message gas only at explicit boundaries; no backend stack-layout claim",
                    "direct typed internal calls and tail calls; model call depth limit 128",
                    "logical product/slice values; boundary leaves in declaration order; pointer-to-word erasure only at the boundary; distinct pointer spaces remain distinct",
                    "aggregate type limits: depth 32, nodes 256; exported type budget 65536; boundary limits: 32 input and 256 return leaves",
                    "explicit ordered FMP allocation; invalid infallible bumps unsupported",
                    "deferred allocations require explicit per-function instruction-site placements and matching extents; static reservations keep FMP and memory unchanged unless zero initialization is requested; heap placements bump on every execution",
                    "slot hashing preserves scratch traffic; storage bytes retain header checks, padding masks and tail clearing; storage work limit 2048 words",
                    "storage arrays preserve scalar encodings, enum checks and nested object order; work limit 2048 elements",
                    "external frame slots use convention-defined addresses; internal frame slots and address queries use explicit bases indexed by the function's live activation count; missing bases are unsupported; implicit typed-call gas requires caller boundary budgets and EIP-150",
                    "external account reads use live state; codecopy/codesize require an explicit code image; literal data patches use supplied library addresses and deferred copies require linked extents; known data must agree with a supplied image; export budget 64 KiB",
                    "message_gas supplies ordered pre-message caller budgets; memory growth, account/value/initcode charges, EIP-150 clipping, stipend and unused child gas are modeled; absent message_gas requires explicit requested CALL gas and assumes sufficient caller gas",
                    "CREATE child gas follows message_gas or explicit creation_gas boundary inputs; abstract MIR instructions remain unpriced",
                    "precompiles use extcall with explicit input staging and scratch-word output; child gas follows call operands or ordered message_gas caller budgets; scratch, returndata and child gas failures remain observable",
                    "compiler-generated check messages follow the fixture's debug_reverts setting; user require payloads are already explicit in MIR",
                    "child bytecode executes with fork-specific gas; gas failure is not assumed away",
                    "returndata, account lifetime and access warmth remain live at internal returns",
                    "recursive ABI encoder/decoder layouts; depth 32, nodes 256 and work 2048; memory/calldata/returndata source locations; bounded packed encoding layouts",
                    "outer returns expose logical components; expanded memory returns require explicit per-function return-buffer placements; a placement selects leaf-ordered physical publication for logical aggregates and slices; unplaced single results stay logical; stored extras must be defined; tail calls retain the original return owner; no backend stack-return or placement claim",
                    "packed literals, scalars, byte references and arrays preserve ordered memory effects; limit 2048 parts, array work 2048 and 64 KiB",
                ]
            )
            if "mir_layout" in fixture:
                assumptions.extend(
                    [
                        "explicit MIR link/placement window inputs; data, library and immutable declaration tables must match; structural SSA types may lower away; no claim about backend placement or runtime image correspondence",
                        "scalar immutables alias constructor staging memory or read fixed runtime relocations; narrow staging words are zero-extended; signed runtime values may be sign-extended; typed reads keep carrier bits and fixed-byte padding stays intact; invalid context words are unsupported; GAS only samples a single-opcode window after its own charge",
                    ]
                )
        else:
            assumptions.extend(
                [
                    "compiled full entry; trusted compiler backend lowering",
                    "only listed symbolic storage cells and entry stack words are quantified",
                    f"entry gas budget {fixture['gas']}; root value zero; origin {fixture['origin']}; gas price {fixture['gas_price']}; fresh private memory",
                    "nested bytecode, constructors and active precompiles execute; fork-specific deferred SELFDESTRUCT",
                    "persistent effects only"
                    if policy is None
                    else "exact effects, remaining gas and refund"
                    if policy == "exact"
                    else "same effects/refund and no less remaining gas unless original runs out of gas",
                ]
            )
    else:
        if program and program.scope in {"mint-layout", "burn-layout"}:
            assumptions.extend(
                [
                    f"Cancun bytecode ABI entry selected by {program.selector}; writable frame with empty stack, memory and returndata",
                    "all calldata bytes, value, initial journals and incoming gas under that entry contract",
                    "all five terminal paths proved from PC zero; instruction charges, SSTORE sentry and frame rollback checked",
                    "same funded effects and no less remaining gas; original out-of-gas may become a funded exit",
                ]
            )
        elif program and program.scope == "allocation-prefix":
            assumptions.extend(
                [
                    "explicit allocation-producer contract: initial FMP is aligned, at least 128, equals its memory word, and the total reserved extent does not wrap uint64; initial active memory is aligned and includes bytes 0x40..0x5f",
                    "arbitrary byte contents, live memory size and allocation cursor; same final cursor, active memory, ordered byte writes and call arguments before an identical continuation",
                    "callee declarations and call suffix are unchanged; arbitrary continuation includes loops, failure and state effects, with no termination or successful-callee assumption",
                    "trusted compiler parser/schema export, SSA-to-byte-memory projection and producer/continuation binding; this is a conditional prefix certificate, not full-state interpreter import correspondence or a proof that every caller establishes the producer contract",
                    "no physical gas, code-image or backend placement claim",
                ]
            )
        elif program and program.scope == "unused-type-erasure":
            assumptions.extend(
                [
                    "complete canonical module code and declaration order retained; only the entire unused named-type table is erased",
                    "independently parsed whole modules retain every function and resolve parameter, result, operand and operation types; kernel checks resolved program data and retained source text for equality",
                    "arbitrary interpreter, complete live state and execution bound; calls, failure and cycles are retained rather than unrolled or assumed successful",
                    "trusted independent compiler parser/schema export and typed constructor renderer; named references are resolved before constructing the semantic view",
                    "structural resolved-module identity, not primitive instruction implementations, physical gas or backend layout; normal GAS execution restrictions remain unchanged",
                ]
            )
        elif program and program.scope == "phase-advance":
            assumptions.extend(
                [
                    "complete canonical MIR retained after removing the lowered phase marker",
                    "compiler validates both original modules at their declared phases",
                    "kernel-checked equality of independently resolved function tables preserves direct MIR execution for every entry, state and work bound",
                    "trusted compiler parser/schema export and typed constructor renderer; backend layout and physical gas remain outside direct MIR execution",
                ]
            )
        elif program and program.scope == "phase-rewrite":
            assumptions.extend(
                [
                    "same module declarations and function interfaces after pointer-to-word representation erasure",
                    "trusted compiler parser/schema export and typed constructor renderer; no physical gas or backend layout claim",
                ]
            )
            certificates = program.import_evidence["certificates"].values()
            kinds = {certificate["kind"] for certificate in certificates}
            if "carrier-graph" in kinds:
                assumptions.extend(
                    [
                        "defined scalar carriers; checked copy availability on every loop and phi edge, with equivalent ordered effects and live state at every logical block bound",
                        "same handlers for retained requests, including GAS and constructor queries; physical gas and placement are not derived",
                    ]
                )
                if any(certificate.get("abort_tails") for certificate in certificates):
                    assumptions.append(
                        "typed straight-line revert helpers execute with the same arguments and never resume their incoming continuation"
                    )
            if "direct-execution" in kinds:
                assumptions.append(
                    "direct execution quantifies defined scalar inputs and arbitrary aligned byte memory up to 64 KiB"
                )
            if "phi-edges" in kinds:
                assumptions.append(
                    "empty forwarding blocks preserve control and simultaneous phi assignment; work bounds count logical edges"
                )
        elif program and program.scope == "empty-dispatch":
            assumptions.extend(
                [
                    "semantic MIR without an explicit entry or any selector, receive or fallback route",
                    "argument-free runtime entry, positive call-depth bound and at most 64 KiB of initial memory",
                    "every calldata, call value and journal; the synthesized MIR entry reaches an empty revert within the proved work bound",
                    "retained function data and constructor root unchanged; physical gas and backend layout are outside this proof",
                ]
            )
        elif program and program.scope == "routed-dispatch":
            hoisted = "minimum_calldata" in program.import_evidence
            library = "library_guarded_routes" in program.import_evidence
            debug = program.import_evidence.get("debug_reverts", False)
            assumptions.extend(
                [
                    "explicit selector wrappers differ only in the first guard branch"
                    if hoisted
                    else "explicit ABI wrappers with unchanged executable bodies",
                    "initial memory at most 64 KiB and calldata length representable as a word",
                    "routing opcode availability checked against the target, with semantic MIR's constant selector shift admitted before lower-shifts"
                    if program.import_evidence.get("legacy_selector_shift")
                    else "routing opcode availability checked against the target",
                    "symbolic calldata and call value; short selected inputs match the original empty revert; valid inputs reach the same wrapper cursor and complete frame"
                    if hoisted
                    else "symbolic calldata and call value; exact wrapper entry frame or the configured Error(string) reason proved by direct MIR execution"
                    if debug
                    else "symbolic calldata and call value; exact wrapper entry frame or empty revert proved using direct MIR transitions",
                    "execution beyond the checked wrapper cut, backend placement and physical gas are outside the routing certificate",
                ]
            )
            if library:
                assumptions.append(
                    "library routing admits call value and compares the executing address with a symbolic deployed address bound to the runtime immutable"
                )
            if program.import_evidence.get("legacy_selector_shift"):
                assumptions.append(
                    "pre-Constantinople selector shifts use semantic MIR; the selector expression equals division by 2^224"
                )
        elif program and program.scope == "fresh-heap-function":
            assumptions.extend(
                [
                    "conditional direct typed SSA proof of the complete function, including all suffix instructions and returns; model errors on either side prevent verification",
                    "explicit producer obligation: the named supplied heap floor equals the live FMP word; initial aligned byte memory includes bytes 0x40..0x5f; floor is aligned and at least 128; the source reservation extent fits the 64 KiB work domain",
                    "arbitrary memory contents, journal, live returndata and execution context satisfying that obligation; no sampled cursor, zeroed heap or synthesized placement",
                    "trusted compiler parser/schema export and typed SSA constructor renderer; no proof that every caller or the backend establishes the placement/FMP obligation",
                    "semantic effects only; abstract MIR is unpriced; no physical gas or backend layout claim",
                ]
            )
        elif program and program.scope == "compact-pushes":
            bound = program.import_evidence["entry_height_bound"]
            if program.import_evidence.get("code_layout"):
                assumptions.append(
                    "constructor reads use the checked sole data suffix and identical appended arguments"
                )
            if program.import_evidence.get("resolved_literal_tables"):
                assumptions.append(
                    "literal indexed transfers use the checked table entry and proven scratch room; emitted table encoding remains a separate obligation"
                )
            if program.import_evidence.get("caller_return_slots"):
                assumptions.append(
                    "verified inherited return slots refer to corresponding logical continuations; a return exposes its slot and live state at the region boundary"
                )
            if program.import_evidence.get("physical_entry"):
                assumptions.extend(
                    [
                        "independent import starts at the first physical block with an empty EVM call-frame stack",
                        "each immutable PUSH reads the same deployment slot and width on both sides",
                    ]
                )
            elif program.import_evidence.get("caller_stack"):
                assumptions.extend(
                    [
                        f"kernel-checked height invariant from both physical entries bounds the selected region's incoming stack by {bound}",
                        "the region proof compares equal incoming stacks and world state at the selected block",
                    ]
                )
            else:
                assumptions.append(
                    f"independently exported EVM region with incoming height at most {bound}"
                )
            assumptions.extend(
                [
                    "each closed recipe produces the same words and preserves the incoming stack; every extra temporary slot is checked",
                    "kernel-checked fork-specific execution charges for the recipes; both executions must be funded through every reached instruction and gas admission check",
                    "position-independent handlers; no unresolved return targets; physical transfer charges are outside the proof",
                ]
            )
            if program.import_evidence.get("interactions"):
                assumptions.extend(
                    [
                        "checked continuation provenance; relocation placeholders cannot appear in literal recipes or in interaction operands",
                        "kernel-checked equality of every interaction prefix under shared preceding replies",
                        "actual GAS and code-size replies, forwarded gas, and child behavior remain environment obligations",
                    ]
                )
            else:
                assumptions.append(
                    "gas-insensitive handlers; GAS, code-image observers and external messages excluded"
                )
        elif program and program.scope == "stack-layout":
            assumptions.extend(
                [
                    "independently exported reachable graphs matched under a node renaming; every opcode, operand, terminal and edge retained",
                    "kernel-checked per-node height bounds from the opcode schema; changed jump stack-growth checks must both be proved safe",
                    "every initial height, live payload and execution bound under arbitrary shared schema-shaped handlers; early failures and stateful cycles retained; no sampled entry height or world",
                    "height is architectural state and opcode effects follow schema arities; operand values, memory, accounts and resource state remain in the arbitrary live payload; opcode-handler/height binding remains trusted",
                    "dynamic JUMP terminals expose the full live payload under a shared logical continuation; no indirect-target reachability or physical return-address correspondence claim",
                    "transfers are unpriced; source normalizer, raw constructor and schema binding remain trusted; no physical gas, opcode-implementation or backend-layout certificate; code-image observers and calls excluded",
                ]
            )
        elif program and program.scope == "layout-flow":
            if program.import_evidence.get("physical_entry"):
                assumptions.append(
                    "independent import starts at the physical EVM entry with an empty stack"
                )
            else:
                bound = program.import_evidence["entry_height_bound"]
                assumptions.extend(
                    [
                        f"kernel-checked height invariant from both physical entries bounds the region's incoming stack by {bound}",
                        "the region compares equal live-in ordinary words and world state; inherited return slots denote corresponding logical continuations",
                    ]
                )
            assumptions.extend(
                [
                    "logical paths preserve ordinary actions and resolve continuation tags; the kernel checks reachable stack bounds for every changed jump",
                    "shared position-independent handlers and immutable slots; interaction requests match under shared preceding replies",
                    "physical gas, forwarded gas and child behavior remain environment obligations",
                ]
            )
            if program.import_evidence.get("code_layout"):
                assumptions.append(
                    "constructor reads use the checked sole data suffix and identical appended arguments"
                )
        elif program and program.scope in {
            "cfg-flow",
            "continuation-flow",
            "interaction-flow",
        }:
            assumptions.extend(
                [
                    "independently exported EVM graphs; matched finite chains of unconditional transfers preserve the strongest temporary stack requirement",
                    "every incoming stack height, arbitrary live state and logical execution bound under shared position-independent handlers; changed growth requirements are proved safe on reachable paths",
                    "compiler parser/schema and constructor bindings are trusted",
                ]
            )
            if program.scope == "interaction-flow":
                assumptions.extend(
                    [
                        "kernel-checked equality of every interaction prefix with shared replies to preceding requests; temporary stack checks and relocated continuations are retained",
                        "observes requested gas, destinations, value, calldata and memory ranges at the opcode boundary before admission",
                        "ordinary opcode handlers and the incoming state are shared; actual code-size and GAS replies, child behavior, reentry and forwarded gas remain environment obligations",
                    ]
                )
            elif program.scope == "continuation-flow":
                assumptions.extend(
                    [
                        "checked continuation provenance through stack operations; dynamic returns consume a known label; ordinary effect operands cannot observe relocation tags",
                        "code-image observers and external messages are excluded; GAS uses a shared handler; physical address and gas equivalence remain outside the certificate",
                    ]
                )
            else:
                assumptions.append(
                    "raw transfer costs and physical return PCs are outside this projection; dynamic JUMP is a terminal observation, and code-image observers, escaping labels and calls are excluded"
                )
        elif program and program.scope == "mir-cfg-flow":
            assumptions.extend(
                [
                    "independently exported typed MIR; kernel-checked edges, literal branch selection, live operand agreement and simultaneous phi writes",
                    "every logical action bound and shared live state; direct callee graphs checked with the same bindings; GAS and message effects are retained requests",
                    "trusted compiler structural exporter and action-constructor binding; no physical gas pricing, ABI frame initialization or runtime callback binding",
                ]
            )
            if any(
                any(details["product_initialization"])
                for details in program.import_evidence["correspondence"].values()
            ):
                assumptions.append(
                    "flat products seeded by undef have kernel-checked initialized fields; reads outside their insert chains require a complete product"
                )
            functions = program.import_evidence["correspondence"].values()
            if any(details.get("word_rewrites") for details in functions):
                assumptions.append(
                    "kernel checks wrapped i256 add/subtract operands at matched actions"
                )
            if any(details["proof"] == "carrier-copies" for details in functions):
                assumptions.append(
                    "both callee graphs check full-word carrier copies against the common erasure; overwrites invalidate captured values across loops"
                )
            if program.import_evidence.get("nested_calls"):
                assumptions.append(
                    "direct calls execute the checked callee graph and pass its result to shared entry and resume handlers, for every logical work and call-unfolding bound"
                )
        elif program and program.scope == "source-identity":
            assumptions.extend(
                [
                    "exact identity of independently exported reachable EVM graphs, including entry, node labels, all operations, terminals, edges and transfer stack-growth checks",
                    "every initial state and execution bound under arbitrary shared node handlers, including stateful effects, early failures and resource state; no sampled stack, world or successful-execution assumption",
                    "structural source certificate; Rust parser/CFG normalizer, constructor binding and interpretation of nodes remain trusted; no opcode-implementation or physical transfer-pricing proof",
                    "code-image observers and calls excluded; logical labels and shared declarations retained; no backend layout, physical gas or whole-pass claim",
                ]
            )
        elif program and program.scope == "function-pruning":
            assumptions.extend(
                [
                    "complete compiler-validated MIR modules; every retained declaration and named reference is unchanged; all external entries remain and retained call targets are closed",
                    "symbolic function identities; arbitrary shared primitive step, mutable state, caller continuations and terminal observations; exact equality for every step bound, including cycles and recursive calls",
                    "structural named-call-machine certificate; compiler parser/display, reference extraction and machine binding remain trusted",
                    "no instruction-semantics, physical gas, numeric function-pointer or backend layout claim; removed functions are not valid entry points",
                ]
            )
        elif program and program.scope == "shared-prefix-suffix":
            assumptions.extend(
                [
                    "exact independently exported common instruction prefix is passed identically to a shared state/gas executor; its interpreter binding remains trusted",
                    "prefix GAS reads, precompile execution, memory, returndata and every early halt are preserved; no prefix instruction is deleted or replaced by entry defaults",
                    "prefix STATICCALLs require canonical push-one/GAS targets for the always-active recovery precompile; arbitrary external calls, relocations and code-image observers excluded",
                    "suffix quantifies every incoming stack, context and live world, including stack underflow and the 1024-word limit",
                    "kernel-proved suffix interaction refinement preserves every original success, exit and complete stack/world with no less gas; suffix handlers and continuation are gas-insensitive",
                    "universally quantified shared per-label PUSH/JUMPI charges; identical terminal and physical successor; logical relocation context, no physical code-placement or whole-contract claim",
                ]
            )
        elif program and program.scope == "effectful-prefix":
            assumptions.extend(
                [
                    "all incoming stack lengths and words; exact 1024-word stack limits",
                    "same ordered opcode requests and arguments; arbitrary replies, aliasing, state changes, effect failures and dynamic charges",
                    "kernel-proved per-interaction scalar cost refinement; every original success preserves the entire stack/world with no less gas",
                    "generic gas-independent effect handlers; calls, code/GAS observers and gas-sensitive storage writes excluded",
                    "trusted stack/effect renderer and handler correspondence, not a full-state interpreter correspondence proof",
                    "identical complete suffix, terminal and successor; gas-insensitive continuation; logical relocation context, no physical code-placement or whole-contract claim",
                ]
            )
            if program.import_evidence.get("conditional_transfers"):
                assumptions.extend(
                    [
                        "adjacent static label PUSH/JUMPI pairs retain taken exits, live stacks and temporary label stack slots",
                        "universally quantified shared per-label PUSH/JUMPI charge context; concrete relocation/price correspondence remains trusted",
                    ]
                )
        elif program and program.scope == "builtin-lowering":
            assumptions.extend(
                [
                    "selected typed SSA and exact lowering operands, phis and successors checked against the proved expansion",
                    "trusted compiler parser/schema export and typed SSA expansion matching; compositional semantic proof for one function",
                ]
            )
            if program.gas == "exact-caller-continuation":
                assumptions.extend(
                    [
                        "final emitted bytes and the MIR snapshots come from the same compiler input and capture",
                        "shared helper-entry environment, stack arguments, byte memory, journal and returndata; the token's code is available to the MIR model",
                        "the typed MIR prefix and final-bytecode effects produce equal memory, target, input, journal and returndata at CALL",
                        "CALL budget derived from the checked byte prefix; the earlier MIR sample clips to the same child gas",
                        "Cancun, at most 30 million entry gas and word-aligned active memory within 64 KiB; the FMP slot is live and the object fits",
                        "the entry stack has a valid return destination and room for the decoded helper's peak temporary depth",
                        "the resolved helper continuation uses decoded opcode prices and memory expansion; MIR has no physical instruction gas",
                        "the five named caller suffixes start with matching saved MIR values and physical stack words; revert uses the checkpoint of the owning invocation",
                        "execution through ABI RETURN, STOP, or owning-frame rollback; caller work before entering the helper is outside the universal proof",
                    ]
                )
                if program.import_evidence.get("final_bytecode", {}).get("abi_entry"):
                    assumptions.append(
                        "the separate ABI-to-exit certificate connects both MIR bodies and emitted bytecode for five concrete caller worlds"
                    )
            else:
                assumptions.append(
                    "unpriced MIR steps; no caller instruction gas, physical layout or whole-pass claim"
                )
            if program.import_evidence["kind"] == "contextual-call-returndata-lowering":
                assumptions.extend(
                    [
                        "same GAS producer in an identical prefix; shared sampled gas word and parent CALL budget at the message boundary, with the same child work bound",
                        "real child execution, gas clipping, failure, return buffer and live memory preserved within the message model",
                        "returned byte length plus 63 fits a word; same typed continuation and supplied callees consume equal results",
                    ]
                )
            elif program.import_evidence["kind"] == "contextual-signing-expansion":
                assumptions.extend(
                    [
                        "shared immutable binding and defined typed operands in the unchanged MIR context",
                        "signing envelope preserves the exact writes, active memory and free-memory pointer update",
                        "STATICCALL uses a paid GAS sample covering its incoming message budget; EIP-150 and available STATICCALL; up to 30 million sample gas",
                        "local recovery result, scratch memory, returndata, journal and remaining message gas; physical gas before the sample is outside the proof",
                    ]
                )
            elif program.gas == "exact-caller-continuation":
                assumptions.append(
                    "bounded returndata materialization; same child effects, copied memory, canonical boolean check and helper exit"
                )
            else:
                assumptions.extend(
                    [
                        "arbitrary storage header, defined object pointer and live memory; ordered writes and scratch hashing, wrapping slot addresses and exact panic payloads",
                        "at most 2048 words per storage loop and 64 KiB active memory; unsupported states outside the model are excluded",
                        "enough MIR internal call depth for the introduced nonrecursive clear helper; physical SSTORE gas and sentry outside this certificate",
                    ]
                )
        elif program and program.scope == "calldata-induction":
            assumptions.extend(
                [
                    "complete typed SSA matched to the index and pointer templates",
                    "arbitrary calldata and shared initial frame under the unpriced typed CFG projection",
                    "loop-header induction and termination bound derived from the ABI guard",
                    "trusted compiler parser/schema export, SSA normalization and CFG projection",
                    "function-local proof; physical gas, stack scheduling and whole-pass correctness outside scope",
                ]
            )
        elif program and program.scope == "memory-induction":
            assumptions.extend(
                [
                    "complete typed SSA executed by the direct MIR interpreter",
                    "shared aligned heap floor at least 128 with room for 224 bytes in 64 KiB",
                    "arbitrary aligned initial byte memory within 64 KiB; expansion, ordered word stores, full memory and ABI output compared",
                    "terminates within 128 logical work units; physical gas and backend placement outside scope",
                ]
            )
        elif program and program.scope == "bounded-fill":
            assumptions.extend(
                [
                    "complete typed CFG matched through ABI dispatch, allocation and return",
                    "common heap placement and arbitrary shared state and memory",
                    "allocation guards derive the sentinel bound for arbitrary runtime counts",
                    "ordered effects, first failure and exit preserved at every logical iteration",
                    "trusted compiler parser/schema export and typed CFG projection",
                    "unpriced source MIR",
                ]
            )
        elif program and program.scope == "storage-induction":
            assumptions.extend(
                [
                    "search preheader to the common packing continuation or the shared failure path",
                    "equal base, needle, initial state and placement of the clear-edge allocation",
                    "ordered storage reads and writes for arbitrary replies and wrapping slots",
                    "trusted compiler parser/schema export, SSA normalization and typed CFG projection",
                    "unpriced region proof scoped to the search and shift loops",
                ]
            )
        elif program and program.scope == "dead-ssa":
            assumptions.extend(
                [
                    "deletion-only SSA stuttering simulation; all retained actions, simultaneous phi groups, predecessor identities and transfers preserved",
                    "erased results are absent from every retained read; only total scalar definitions and typed slice construction/projection can disappear",
                    "slice construction reads defined pointer and length words without dereferencing; projections preserve the selected field type and address space; explicit undef is excluded",
                    "every initial register file and shared state, arbitrary failing effect handlers and every logical block-transfer bound; cycles need no fixed unrolling or termination assumption",
                    "trusted independent compiler parser/schema export, injective local-ID normalization, total-pure classification and action renderer; supplied callees unchanged and cannot re-enter the changed body",
                    "contextual action-machine proof, not direct typed-interpreter correspondence; equal finite-budget errors are not the verification criterion",
                    "unpriced scalar work and block transfers; GAS, code observers, external calls and placement queries excluded; SSTORE failures retained but physical gas and the EIP-2200 sentry's scalar-work costs are not certified",
                ]
            )
        elif program and program.scope == "effect-tree":
            assumptions.extend(
                [
                    "acyclic integer/pointer MIR effect tree from the independently parsed source; trusted constructor renderer",
                    "every ordered opcode/allocation/constructor-range request, operand, arbitrary result word and terminal boundary is preserved",
                    "all stateful effect handlers, including early failure and arbitrary aliasing; no empty-world or successful-access assumption",
                    "compositional interaction certificate, not a full-state interpreter proof; supplied acyclic internal bodies expand with private SSA locals and shared effect state; normal returns resume the caller, frame-ending outcomes do not",
                    "allocations preserve size, zeroing and checked-failure policy; no successful allocation, freshness, implicit FMP or backend return placement is assumed; FMP reads use the kernel-proved MLOAD(64) request; constructor base/end remain distinct arbitrary requests without supplied placements; external calls, other layout queries, GAS sampling and code observers excluded",
                    "SSTORE remains a request that may fail; scalar gas costs and their effect on the EIP-2200 sentry are outside this unpriced MIR certificate",
                    "abstract scalar steps unpriced; no physical gas, backend layout or whole-contract claim",
                ]
            )
        elif program and program.symbolic_mir is not None:
            assumptions.extend(
                [
                    "direct typed SSA source execution; supplied acyclic call graph; no backend lowering, implicit FMP or ABI entry setup",
                    "all typed argument leaves and arbitrary initial journal, memory contents, return data and caller environment",
                    "initial memory is aligned and at most 64 KiB; unproved accesses, model errors and work exhaustion cannot verify",
                    "all returned leaves, live memory, persistent/transient storage, access warmth and returndata remain observable",
                    "abstract MIR operations unpriced; no physical gas, backend placement or whole-contract claim",
                ]
            )
        elif program and program.scope == "interior-window":
            assumptions.extend(
                [
                    "independently exported identical complete prefix and suffix, including terminal and physical successor; shared context-executor bindings remain trusted",
                    "arbitrary prefix exits, storage, logs, memory, gas and live context are retained; no successful-prefix or entry-default assumption",
                    "scalar island preserves every incoming stack and exact remaining gas, including underflow, overflow and out-of-gas; gas-sensitive storage sentries in the shared context remain observable",
                    "code-image observers, external calls and Amsterdam two-resource gas excluded; logical local certificate, not full-state importer correspondence or physical code-placement proof",
                ]
            )
        elif program and program.scope == "prefix-window":
            assumptions.extend(
                [
                    "all incoming stack lengths and values; exact 1024-word overflow and underflow checks",
                    "effect-free fixed-price prefix; identical complete logical continuation; no code-image or interprocedural claim",
                    "arbitrary live world; no zero-memory or empty-storage assumption",
                    "arbitrary 160-bit caller and calldata bytes; size words are independently quantified, a conservative superset of real frame inputs",
                    "arbitrary 256-bit call value",
                    "all gas budgets and gas-sensitive continuation"
                    if program.gas == "exact"
                    else "sufficient gas; resource refinement and gas-insensitive continuation",
                ]
            )
            if program.import_evidence.get("shared_prologue_operations"):
                assumptions.extend(
                    [
                        "identical memory prologue executes with dynamic and expansion charges; its faults and all live memory are preserved",
                        "sufficient gas for the scalar rewrite after every successful prologue execution"
                        if program.gas != "exact"
                        else "all gas budgets, including faults within the shared memory prologue",
                    ]
                )
        elif program and program.scope == "universal-region":
            assumptions.extend(
                [
                    "complete acyclic EVM region to terminal outcomes; all incoming stack lengths and values, including exact overflow checks",
                    "arbitrary projected memory; no assumed incoming constants, dynamic or escaping labels, code-image observations or physical gas claim",
                    "arbitrary 160-bit caller and calldata bytes; size words are independently quantified, a conservative superset of real frame inputs",
                    "arbitrary 256-bit call value",
                ]
            )
        elif layer == "mir":
            assumptions.append("acyclic scalar MIR; no implicit backend frame setup")
        else:
            assumptions.append(
                "cyclic source graph correspondence for every step bound and initial projected state; no fixed unrolling or termination assumption; no dynamic/escaping labels or physical gas claim"
                if program and program.scope == "cyclic-region"
                else "complete acyclic EVM region to terminal outcomes; no dynamic or escaping labels; no PC/CODESIZE; sufficient gas"
                if program and program.scope == "region"
                else "one opcode window; explicit valid label transfers; no PC/CODESIZE"
            )
            if program:
                assumptions.append(f"entry stack height {program.interface[1]}")
        if (
            program
            and program.scope
            and program.scope
            not in {
                "function-pruning",
                "unused-type-erasure",
                "phase-advance",
                "phase-rewrite",
                "empty-dispatch",
                "routed-dispatch",
                "allocation-prefix",
                "calldata-induction",
                "memory-induction",
                "bounded-fill",
                "storage-induction",
                "dead-ssa",
                "builtin-lowering",
            }
        ):
            assumptions.append(
                "function-local proof with supplied callees; no whole-pass claim"
                if program.symbolic_mir is not None
                else "region-local proof over resolved internal return paths"
                if program.scope in {"continuation-flow", "interaction-flow"}
                else "selected entry and reachable direct callees in shared logical frames"
                if program.scope == "mir-cfg-flow"
                and program.import_evidence.get("nested_calls")
                else "region-local proof; no whole-pass or interprocedural claim"
            )
        if (program and program.memory) or (
            result.pair and any(side.memory for side in result.pair)
        ):
            assumptions.append(
                "constant accesses inside 256-byte memory prefix; representable aligned MSIZE"
            )
        if (
            not program
            or program.scope
            not in {
                "prefix-window",
                "effectful-prefix",
                "shared-prefix-suffix",
                "function-pruning",
                "unused-type-erasure",
                "phase-advance",
                "phase-rewrite",
                "empty-dispatch",
                "routed-dispatch",
                "calldata-induction",
                "memory-induction",
                "bounded-fill",
                "storage-induction",
                "dead-ssa",
                "builtin-lowering",
                "allocation-prefix",
                "mint-layout",
                "burn-layout",
                "interior-window",
                "compact-pushes",
                "source-identity",
                "stack-layout",
                "cfg-flow",
                "continuation-flow",
                "interaction-flow",
                "layout-flow",
                "mir-cfg-flow",
            }
            and program.symbolic_mir is None
        ):
            assumptions.append(
                "sufficient gas"
                if policy is None
                else "exact remaining gas and outcomes"
                if policy == "exact"
                else "resource refinement; original OOG unconstrained; gas-insensitive continuation"
            )
    sources = []
    for i, path in enumerate((before, after)):
        sources.append(
            {
                "path": str(path.resolve()),
                "canonical_sha256": hashlib.sha256(
                    result.pair[i].canonical_ir.encode()
                ).hexdigest()
                if result.pair
                else None,
                "module_sha256": hashlib.sha256(
                    (result.pair[i].replay_ir or result.pair[i].canonical_ir).encode()
                ).hexdigest()
                if result.pair
                else None,
            }
        )
    return {
        "version": 1,
        "status": result.status,
        "layer": layer,
        "semantics": model or semantics(),
        "target": target,
        "gas_policy": (
            "exact-caller-continuation"
            if program and program.gas == "exact-caller-continuation"
            else "refine"
            if program and program.scope in {"mint-layout", "burn-layout"}
            else "unpriced-shared-interactions"
            if program
            and (
                program.scope == "interaction-flow"
                or program.scope == "layout-flow"
                and program.import_evidence["interactions"]
            )
            else "unpriced-structural"
            if program
            and program.scope
            in {
                "function-pruning",
                "unused-type-erasure",
                "phase-advance",
                "phase-rewrite",
                "empty-dispatch",
                "routed-dispatch",
                "calldata-induction",
                "memory-induction",
                "bounded-fill",
                "storage-induction",
                "dead-ssa",
                "builtin-lowering",
                "allocation-prefix",
                "source-identity",
                "stack-layout",
                "cfg-flow",
                "continuation-flow",
                "layout-flow",
                "mir-cfg-flow",
                "return-path",
            }
            else program.gas
            if program
            and program.scope
            in {
                "prefix-window",
                "effectful-prefix",
                "shared-prefix-suffix",
                "interior-window",
                "compact-pushes",
            }
            else policy
        )
        or (
            "semantic-effects"
            if program
            and (
                program.symbolic_mir is not None
                or program.world
                and program.world.get("direct_mir")
            )
            else "fixture-effects"
            if program and program.world
            else "sufficient"
        ),
        "assumptions": assumptions,
        "inputs": sources,
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "function",
                    "proof": "producer-valid-allocation-prefix",
                },
            }
            if program and program.scope == "allocation-prefix"
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "module",
                    "proof": "rooted-function-table-correspondence",
                    "retained": program.import_evidence["retained"],
                    "removed": program.import_evidence["removed"],
                    "external_entries": program.import_evidence["external_entries"],
                },
            }
            if program and program.scope == "function-pruning"
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "module",
                    "proof": "resolved-type-table-erasure",
                    "removed_types": program.import_evidence["removed_types"],
                    "functions": program.import_evidence["functions"],
                },
            }
            if program and program.scope == "unused-type-erasure"
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "module",
                    "proof": "resolved-phase-advance",
                    "phases": program.import_evidence["phases"],
                    "functions": program.import_evidence["functions"],
                },
            }
            if program and program.scope == "phase-advance"
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "module",
                    "proof": "resolved-phase-rewrites",
                    "functions": program.import_evidence["functions"],
                    "certificates": program.import_evidence["certificates"],
                },
            }
            if program and program.scope == "phase-rewrite"
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "module",
                    "proof": "empty-runtime-dispatch",
                    "dispatcher": program.import_evidence["dispatcher"],
                    "retained": program.import_evidence["retained"],
                    "work_steps": program.import_evidence["work_steps"],
                },
            }
            if program and program.scope == "empty-dispatch"
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "module",
                    "proof": "debug-runtime-dispatch"
                    if program.import_evidence.get("debug_reverts")
                    else "calldata-guard-hoist"
                    if "minimum_calldata" in program.import_evidence
                    else "library-runtime-dispatch"
                    if "library_guarded_routes" in program.import_evidence
                    else "routed-runtime-dispatch",
                    **{
                        key: program.import_evidence[key]
                        for key in (
                            "dispatcher",
                            "retained",
                            "work_steps",
                            "selectors",
                            "receive",
                            "fallback",
                            "minimum_calldata",
                            "library_guarded_routes",
                            "debug_reverts",
                            "legacy_selector_shift",
                        )
                        if key in program.import_evidence
                    },
                },
            }
            if program and program.scope == "routed-dispatch"
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "window",
                    "name": program.selector,
                    "proof": "universal-exact-scalar-island"
                    if program.scope == "interior-window"
                    else "universal-post-precompile-suffix"
                    if program.scope == "shared-prefix-suffix"
                    else "universal-branching-effect-prefix"
                    if program.import_evidence.get("conditional_transfers")
                    else "universal-effectful-stack-prefix",
                    "stack_height": None,
                },
            }
            if program
            and program.scope
            in {"effectful-prefix", "shared-prefix-suffix", "interior-window"}
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "entry"
                    if program.import_evidence.get("physical_entry")
                    else "region",
                    "name": program.selector,
                    "proof": "kernel-checked-schema-stack-layout"
                    if program.scope == "stack-layout"
                    else "kernel-checked-jump-chain-projection"
                    if program.scope == "cfg-flow"
                    else "kernel-checked-continuation-flow"
                    if program.scope == "continuation-flow"
                    else "kernel-checked-interaction-trace"
                    if program.scope == "interaction-flow"
                    else program.import_evidence["kind"]
                    if program.scope in {"compact-pushes", "layout-flow"}
                    else "universal-source-graph-identity",
                    "stack_height": None,
                },
            }
            if program
            and program.scope
            in {
                "source-identity",
                "stack-layout",
                "cfg-flow",
                "continuation-flow",
                "interaction-flow",
                "compact-pushes",
                "layout-flow",
            }
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "return-path",
                    "name": program.selector,
                    "proof": "kernel-checked-bytecode-return"
                    if result.status == "verified"
                    else "kernel-checked-bytecode-witness",
                },
            }
            if program and program.scope == "return-path"
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "function",
                    "name": program.selector,
                    "proof": "kernel-checked-typed-jump-chain",
                    "functions": program.import_evidence["functions"],
                },
            }
            if program and program.scope == "mir-cfg-flow"
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "abi-entry",
                    "name": program.selector,
                    "proof": f"kernel-checked-{program.scope.removesuffix('-layout')}-gas-refinement",
                },
            }
            if program and program.scope in {"mint-layout", "burn-layout"}
            else {}
        ),
        **(
            {"import_correspondence": [side.import_evidence for side in result.pair]}
            if result.pair and any(side.import_evidence for side in result.pair)
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "loop-region"
                    if program.scope == "storage-induction"
                    else "function",
                    "name": program.selector,
                    "proof": program.import_evidence["kind"],
                },
            }
            if program
            and program.scope
            in {
                "builtin-lowering",
                "calldata-induction",
                "memory-induction",
                "bounded-fill",
                "storage-induction",
            }
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "function",
                    "name": program.selector,
                    "proof": "cyclic-dead-ssa-simulation",
                    "erased_results": program.import_evidence["erased_results"],
                    "blocks": program.import_evidence["blocks"],
                },
            }
            if program and program.scope == "dead-ssa"
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "function",
                    "name": program.selector,
                    "proof": "compositional-effect-tree"
                    if program.scope == "effect-tree"
                    else "producer-valid-fresh-heap-ssa"
                    if program.scope == "fresh-heap-function"
                    else "universal-caller-state",
                    "work_bounds": [side.symbolic_mir["fuel"] for side in result.pair],
                    "supplied_functions": len(
                        program.symbolic_mir["model"]["functions"]
                    ),
                },
            }
            if program and program.symbolic_mir is not None and result.pair
            else {}
        ),
        **(
            {
                "whole_pass": False,
                "region": {
                    "kind": "region",
                    "name": program.selector,
                    "proof": "universal-stack-region",
                    "stack_height": None,
                },
            }
            if program and program.scope == "universal-region"
            else {}
        ),
        "evidence": "kernel-proof"
        if result.status == "verified"
        else "kernel-and-rust-replay"
        if result.status == "counterexample"
        else None,
        **(
            {
                "world": {
                    "fixture_sha256": program.world["fixture_sha256"],
                    "fixture": {
                        key: value
                        for key, value in program.world.items()
                        if key
                        not in {
                            "configuration",
                            "image_sha256",
                            "compiler",
                            "path",
                            "fixture_sha256",
                        }
                    },
                    "backend_images": [
                        side.world["image_sha256"]
                        for side in result.pair
                        if side.world is not None and "image_sha256" in side.world
                    ]
                    if result.pair
                    else [],
                    "opcode_config_sha256": hashlib.sha256(
                        program.world["configuration"].encode()
                    ).hexdigest(),
                    "native_samples": 1 if result.status == "verified" else 0,
                }
            }
            if program and program.world
            else {}
        ),
        "witness": result.witness,
        "diagnostic": result.stderr,
    }


def context(record: dict) -> str:
    model = record["semantics"]
    return (
        f"semantics: {model['version']} sha256={model['sha256']} ({model['lean']})\n"
        f"target: {record['target']}; gas policy: {record['gas_policy']}\n"
        "assumptions: " + "; ".join(record["assumptions"]) + "\n"
    )


def write(path: Path, record: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w", encoding="utf-8", dir=path.parent, delete=False
        ) as file:
            temporary = Path(file.name)
            json.dump(record, file, indent=2)
            file.write("\n")
        os.replace(temporary, path)
    finally:
        if temporary:
            temporary.unlink(missing_ok=True)


def summarize(arguments: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Summarize solar-tv audit records")
    parser.add_argument("directory", type=Path)
    args = parser.parse_args(arguments)
    totals = dict.fromkeys(STATUSES, 0)
    if not (args.directory / "audits").is_dir():
        parser.exit(2, "no audit directory found\n")
    try:
        for path in sorted((args.directory / "audits").glob("*.json")):
            data = json.loads(path.read_text())
            if (
                not isinstance(data, dict)
                or data.get("version") != 1
                or not isinstance(data.get("status"), str)
                or data["status"] not in totals
            ):
                raise ValueError(f"invalid audit record: {path}")
            totals[data["status"]] += 1
    except (OSError, ValueError, AttributeError) as error:
        parser.exit(2, f"cannot summarize audit: {error}\n")
    print(json.dumps(totals, indent=2))
    return 0
