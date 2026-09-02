function config() {
  const url = process.env.CLICKHOUSE_URL
  if (!url) throw new Error('Missing CLICKHOUSE_URL')
  return {
    url,
    database: process.env.CLICKHOUSE_DATABASE || 'solar_perf',
    user: process.env.CLICKHOUSE_USER || 'default',
    password: process.env.CLICKHOUSE_PASSWORD || '',
  }
}

function headers({ user, password }, contentType = 'text/plain; charset=utf-8') {
  return {
    authorization: `Basic ${Buffer.from(`${user}:${password}`).toString('base64')}`,
    'content-type': contentType,
  }
}

async function request(query, body = query) {
  const settings = config()
  const url = new URL(settings.url)
  url.searchParams.set('database', settings.database)
  const response = await fetch(url, {
    method: 'POST',
    headers: headers(settings),
    body,
  })
  if (response.ok) return response
  throw new Error(`ClickHouse request failed (${response.status}): ${await response.text()}`)
}

export async function execute(query) {
  await request(query)
}

export async function insert(table, rows) {
  if (!rows.length) return
  const body = `${rows.map((row) => JSON.stringify(row)).join('\n')}\n`
  await request(`INSERT INTO ${table} FORMAT JSONEachRow`, body)
}

export async function select(query) {
  const response = await request(`${query}\nFORMAT JSONEachRow`)
  const body = await response.text()
  return body
    .trim()
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line))
}
