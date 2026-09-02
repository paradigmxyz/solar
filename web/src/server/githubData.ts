const commit = '[0-9a-f]{40}'
const benchmark = '[A-Za-z0-9][A-Za-z0-9._-]*'
const artifact = '[0-9]\\.json'

const runDocument = new RegExp(`^runs/${commit}/run\\.json$`)
const artifactDocument = new RegExp(`^runs/${commit}/${benchmark}/(?:solar|solc)/${artifact}$`)

export function isPublishedDataPath(path: string) {
  return path === 'index.json' || runDocument.test(path) || artifactDocument.test(path)
}

export function publishedDataUrl(repository: string, ref: string, path: string) {
  if (!isPublishedDataPath(path)) throw new Error(`Unsafe published data path: ${path}`)
  const [owner, name] = repository.split('/')
  if (!owner || !name || repository.split('/').length !== 2)
    throw new Error(`Invalid GitHub repository: ${repository}`)
  return new URL(
    [owner, name, ref, 'data', ...path.split('/')].map(encodeURIComponent).join('/'),
    'https://raw.githubusercontent.com/',
  ).toString()
}
