# Solar web

This standalone Vite app publishes a benchmark viewer to GitHub Pages. PR benchmark comments link
straight to a comparison. The browser finds the matching GitHub Actions runs, downloads their
benchmark artifacts, and keeps the downloaded ZIPs only in the browser cache.

```bash
pnpm install --frozen-lockfile
pnpm dev
```

Open a comparison URL with full commit SHAs, for example:

```text
http://127.0.0.1:5173/solar/?base=<base-sha>&head=<head-sha>&benchmark=<benchmark>
```

GitHub requires an access token with Actions read access to download artifact ZIPs. The site stores
the token only in browser storage; it never sends it to a Solar service.

Run `pnpm check`, `pnpm test`, and `pnpm build` before changing the site.

## API worker

`src/worker.ts` is an optional Hono Cloudflare Worker for the historical dashboard. It serves
normalized run history, benchmark rows, and artifact content directly from ClickHouse. Start it
with `pnpm worker:dev`, then point the site at it with:

```bash
VITE_WEB_API_URL=http://127.0.0.1:8787 pnpm dev
```

Set the following Worker secrets with Wrangler: `CLICKHOUSE_HOST`, `CLICKHOUSE_USER`, and
`CLICKHOUSE_PASSWORD`. `CLICKHOUSE_DATABASE` defaults to `solar_perf`.

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
VITE_WEB_API_URL=http://127.0.0.1:8787 pnpm dev
```

Stop the database with `docker compose down`. Add `-v` only when you want to discard local data.
