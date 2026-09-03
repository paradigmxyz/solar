function config() {
  const host = process.env.CLICKHOUSE_HOST
  if (!host) throw new Error('Missing CLICKHOUSE_HOST')
  return {
    url: host.startsWith('http://') || host.startsWith('https://') ? host : `https://${host}`,
    database: database(),
    user: process.env.CLICKHOUSE_USER || 'default',
    password: process.env.CLICKHOUSE_PASSWORD || '',
  }
}

export function database() {
  const name = process.env.CLICKHOUSE_DATABASE || 'solar_perf'
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) throw new Error('Invalid CLICKHOUSE_DATABASE')
  return name
}

function headers({ user, password }, contentType = 'text/plain; charset=utf-8') {
  return {
    authorization: `Basic ${Buffer.from(`${user}:${password}`).toString('base64')}`,
    'content-type': contentType,
  }
}

async function request(query, body = query, useDatabase = true) {
  const settings = config()
  const url = new URL(settings.url)
  if (useDatabase) url.searchParams.set('database', settings.database)
  const response = await fetch(url, {
    method: 'POST',
    headers: headers(settings),
    body: body === query ? query : `${query}\n${body}`,
  })
  if (response.ok) return response
  throw new Error(`ClickHouse request failed (${response.status}): ${await response.text()}`)
}

export async function execute(query, useDatabase = true) {
  await request(query, query, useDatabase)
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
