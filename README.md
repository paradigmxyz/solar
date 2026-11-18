# c2: cold replay of the c1b controls on solar 65d87ab3

Receipts only. Driver solar 65d87ab35420acd048ee1015113b6a887ab065ba, harness 8d4bae65a179149c2e05c002399072dc9de72403
(c2.sh), inputs the three control archives of the c1b receipts (commit e5b1ce91755c79a07c934252c5683ed320f7fd22,
receipts.sha256 574ab7c8...), checked against their digests (archives.sha256) before any replay. Each replay is
tools/certify/certify --replay of the control's record with a package cache fetched fresh by lake on this machine.

- outbox/NAME.tgz with NAME.tgz.sha256 (digest taken where it ran, verified on copy): the case directory: status,
  replay.command, replay.log, scope.txt, the c1b record (rec/) and the fresh run's record (rec.replay-*/).
- run-meta.tgz: run.log, table.txt, run-scope.txt, archives.check, archives.sha256, run-manifest.txt (with the full
  command), c2.out, versions.txt, setup.log.
- receipts.sha256: every file here.
