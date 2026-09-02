# Solar performance site

This standalone Vite+ app reads benchmark data through the performance Worker. Local development
uses `https://raw.githubusercontent.com/paradigmxyz/solar/gh-pages/data/` and falls back to
`public/data` until an API URL is configured. Set `VITE_PERF_API_URL` for the Worker or
`VITE_PERF_DATA_URL` for a static archive. The homepage loads only `index.json`; commit results and
compiler artifacts load when a comparison opens.

```bash
pnpm install --frozen-lockfile
pnpm dev
```

Run `pnpm check`, `pnpm test`, and `pnpm build` before changing the site.

## API worker

`src/worker.ts` is a Hono Cloudflare Worker. With ClickHouse configured, it serves normalized run
history, benchmark rows, and artifact content directly from the database. Without ClickHouse it
keeps the immutable `gh-pages` archive as a local-development fallback. Start it with
`pnpm worker:dev`, then point the site at it with:

```bash
VITE_PERF_API_URL=http://127.0.0.1:8787 pnpm dev
```

Set the following Worker secrets with Wrangler: `CLICKHOUSE_URL`, `CLICKHOUSE_USER`, and
`CLICKHOUSE_PASSWORD`. `CLICKHOUSE_DATABASE` defaults to `solar_perf`. `GITHUB_TOKEN`,
`GITHUB_REPOSITORY`, and `PERF_DATA_REF` only configure the static fallback.

## ClickHouse ingestion

[`schema/clickhouse.sql`](schema/clickhouse.sql) stores an immutable raw result document alongside
normalized compiler metrics and compressed artifact contents. The normalizer accepts the current
result schema and older aliases (`id`/`name`, top-level compiler objects, camelCase metrics), so old
runs remain queryable even when benchmark sets change.

Set `CLICKHOUSE_URL`, `CLICKHOUSE_USER`, and `CLICKHOUSE_PASSWORD`, then create the schema:

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

Set the repository variable `PERF_API_URL` to the deployed Worker URL so the Pages build uses
ClickHouse. The legacy `gh-pages` archive remains a fallback while the database is being filled.

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
