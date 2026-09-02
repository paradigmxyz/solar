# Solar performance site

This standalone Vite app publishes a benchmark viewer to GitHub Pages. PR benchmark comments link
straight to a comparison. The static site loads the two run documents and artifact files from its
own `data/` directory; it does not need ClickHouse or a Worker.

```bash
pnpm install --frozen-lockfile
pnpm dev
```

To test the static path against local published data without the Worker, run:

```bash
VITE_PERF_DATA_URL=/solar/data pnpm dev
```

Open a comparison URL with full commit SHAs, for example:

```text
http://127.0.0.1:5173/solar/?base=<base-sha>&head=<head-sha>&benchmark=<benchmark>
```

Run `pnpm check`, `pnpm test`, and `pnpm build` before changing the site.

## API worker

`src/worker.ts` is an optional Hono Cloudflare Worker for the historical dashboard. It serves
normalized run history, benchmark rows, and artifact content directly from ClickHouse. Start it
with `pnpm worker:dev`, then point the site at it with:

```bash
VITE_PERF_API_URL=http://127.0.0.1:8787 pnpm dev
```

Set the following Worker secrets with Wrangler: `CLICKHOUSE_HOST`, `CLICKHOUSE_USER`, and
`CLICKHOUSE_PASSWORD`. `CLICKHOUSE_DATABASE` defaults to `solar_perf`. `GITHUB_TOKEN`,
`GITHUB_REPOSITORY`, and `PERF_DATA_REF` only configure the static fallback.

## ClickHouse ingestion

[`schema/clickhouse.sql`](schema/clickhouse.sql) stores an immutable raw result document alongside
normalized compiler metrics and compressed artifact contents. The normalizer accepts the current
result schema and older aliases (`id`/`name`, top-level compiler objects, camelCase metrics), so old
runs remain queryable even when benchmark sets change.

Set `CLICKHOUSE_HOST`, `CLICKHOUSE_DATABASE`, `CLICKHOUSE_USER`, and `CLICKHOUSE_PASSWORD`, then
create the schema. `CLICKHOUSE_HOST` accepts a bare HTTPS host or a full URL for local ClickHouse:

```bash
pnpm db:schema
```

### Local end-to-end test

Docker Compose starts a local ClickHouse with the schema mounted as an init script. It stores data
in a named Docker volume and only binds ports 8123 and 9000 on localhost. The local default-user
password is `local-dev`; it is only for the Compose instance.

```bash
docker compose up -d --wait
cp .dev.vars.example .dev.vars
set -a && source .dev.vars && set +a
pnpm db:verify
node scripts/ingest-run.mjs \
  --results ../target/codegen-bench/site-preview/3d436956/results.json \
  --artifacts ../target/codegen-bench/site-preview/3d436956/artifacts \
  --commit 3d436956c9fabea5c92e08612bd8d5ade5501a57 \
  --workflow-run 1
pnpm worker:dev --local
```

In a second terminal, start the site against the local Worker:

```bash
VITE_PERF_API_URL=http://127.0.0.1:8787 pnpm dev
```

Stop the database with `docker compose down`. Add `-v` only when you want to discard local data.

`.github/workflows/perf-ingest.yml` imports a completed Benchmark workflow immediately and scans
the retained workflow history every 15 minutes. Run its manual dispatch once to backfill every
GitHub Actions artifact that GitHub still retains. The importer is idempotent by workflow run ID,
records a raw input for later migrations, and skips expired or pre-artifact runs without stopping
the rest of the backfill.

`perf-pages.yml` imports each completed Benchmark workflow into the Pages data directory, retains
the latest 200 runs, and deploys the static viewer. It does not use `PERF_API_URL`.

Import a local benchmark run before starting the site:

```bash
node scripts/import-run.mjs \
  --results ../target/codegen-bench/results.json \
  --artifacts ../target/codegen-bench/artifacts \
  --output public/data \
  --commit "$(git rev-parse HEAD)" \
  --branch "$(git branch --show-current)"
```

Do not commit imported run data. CI keeps it on the `gh-pages` branch and deploys that branch with
GitHub Pages.
