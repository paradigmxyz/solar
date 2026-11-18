# c3a: three negatives on solar 65d87ab3

Receipts only. Driver solar 65d87ab35420acd048ee1015113b6a887ab065ba (build provenance version 2), harness
ec92fe4086edaded1086879cd221061a4759d7ea, solidity-lean 334d1d924f8f6c29ac827fd81d1cc739721b479a. Cases: meterwrongspec,
clampwrongspec-r2 and siblingfns-r2 (c3a.tsv, sha256 4499a976..., asserted on the machine to be exactly those rows of
cases.tsv). Controls joined from the c1b archives (receipts e5b1ce91) and their c2 replays (receipts 1df49bea), each
checked against its digest list (JOIN PASS in join.log).

- outbox/NAME.tgz with NAME.tgz.sha256 (digest taken where it ran, verified on copy).
- run-meta.tgz: run.log, table.md (with the control gate), producer.json/.log, join.log, the joined controls'
  join.json and gate.json, admission.txt, run-manifest.txt (full command), versions.txt, setup.log, run-scope.txt,
  go.log, c3a.tsv.
- receipts.sha256: every file here.
