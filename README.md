# Spotify simple SDK

Created to practice interfacing with the Spotify API. My main goal is to create a simple song tracker than can check what I'm listening to at all times, and track some metrics like minutes listening, favorite songs, and favorite artists.

## Dev docs

- https://developer.spotify.com/documentation/web-api/tutorials/code-pkce-flow
  - https://developer.spotify.com/documentation/web-api/concepts/authorization
  - https://developer.spotify.com/documentation/web-api/concepts/apps
- [OAuth 2.0](https://datatracker.ietf.org/doc/html/rfc6749)
- [PKCE](https://datatracker.ietf.org/doc/html/rfc7636#section-4.1)
- [Bitwarden SDK](https://github.com/bitwarden/sdk), we depend on the git repo because the crate in crates.io is 5 months old and busted.

### Spotify Auth setup Steps

1. App requests user authorization to spotify by generating a URL, user has to paste into Browser and authorize this app.
1. Spotify will redirect the user to a dummy url, but with the response info encoded in the URL as search parameters. `code` and `state`.
   1. Don't forget to add the redirect URI in the APP Spotify management dashboard!!!
   1. Example Redirect:`http://localhost:8080/?code=AQAJQs0ZXTxhvkRUMXn1PVLQQBw2VXSldRqfou5RPM_RPkHdexx7v7lUNcjXjWzPKFW3bxxPLuHCJqoQy6NbIr-70-ZpPszqktjxBgzqqmKLv653gjh_f_-ELVPdWscUvlNlICrcyUGtGPCIIdDLWHg9bVEsBMFtyrEtA8S6bYoUbC-3YhqhNr6GC90rM3AmmTUqhTC2jkINQ9aFMCalO2l34NLE9kXqIVe2hBMaEdOuBNfi3zXhdG0kulgAJ8a03nAVMs9HBJXKFzD5bVFvl7eXj3p6DwMOnQFxFJq9wJHbg57a507DPmVr8vO_nYRcr6uXhVgMEY4WkR0djj3CgeKSUNOVGB-VwUs8YcyZH-kfaUoeOsY-6hyiDUizDPGXorL0vskU7GmTGsat2UwsSkanGeJvr3BP9-GVVIQFcU91WNiG2rkAa8rIWJz_EgRtqco7yA`
   1. If the user does not accept the request, or if there is anything wrong, the response will contain: `error` and `state`.
1. App needs to request for an access token using the `code` returned in the last step.
1. Store the returned `access_token` and `refresh_tokens` from the previous step.

### Specs

- [Spotify Auth Scopes](https://developer.spotify.com/documentation/web-api/concepts/scopes) In Use:
  - `user-read-playback-state`
  - `user-read-currently-playing`
  - `playlist-read-private`
  - `user-read-playback-position`
  - `user-top-read`
  - `user-read-recently-played`
  - `user-library-read`

### Bitwarden Secrets Manager Setup

There are a few values that the script needs in order to bootstrap with Bitwarden secrets manager.

- Create the project in Bitwarden and note down organization id and project id, these are actually not straight forward to find, I copied them from the URL itself while navigating around.
- Generate a machine access token with permissions to read and write secrets in the project.
- Create file `bitwarden_config.json`

```json
{
  "access_token": "<machine code access token>",
  "org_id": "<uuid>",
  "project_id": "<uuid>"
}
```

- Also, within bitwarden, create a secret called `spotify_client_id` with the app client id that spotify grants you when creating a new app.

## Notes to spotify

- The PKCE example doesn't implement PKCE correctly, but it will still work. Its missing the special characters.
- Last step in the [PKCE Tutorial](https://developer.spotify.com/documentation/web-api/tutorials/code-pkce-flow) doesn't mention which URL to target for getting access tokens.
