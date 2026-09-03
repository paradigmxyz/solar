import { type FormEvent, useState } from 'react'
import { setGitHubToken } from './githubActions'

interface Props {
  onSave: () => void
}

export function GitHubAccess({ onSave }: Props) {
  const [token, setToken] = useState('')

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!token.trim()) return
    setGitHubToken(token)
    onSave()
  }

  return (
    <main className="compare-page access-page">
      <p className="eyebrow">GitHub Actions</p>
      <h1>Connect GitHub to load this benchmark</h1>
      <p>
        GitHub requires an access token to download Actions artifacts. The token stays in this
        browser; benchmark data is cached only in this browser.
      </p>
      <form onSubmit={submit} className="github-access">
        <label>
          GitHub token with Actions read access
          <input
            autoComplete="off"
            autoFocus
            onChange={(event) => setToken(event.target.value)}
            spellCheck={false}
            type="password"
            value={token}
          />
        </label>
        <button type="submit">Load benchmark</button>
      </form>
    </main>
  )
}
