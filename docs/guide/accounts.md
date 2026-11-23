# Accounts

Connect **GitHub**, **GitLab** or **Azure DevOps** to browse and clone your repositories, work with pull requests, and give Git a
token for HTTPS remotes. Open **Settings → Accounts → Connect an account…**.

## Connecting

1. Choose the **service**.
2. Name the **host** if you use your own server (GitHub Enterprise, self-hosted GitLab), or the **organization** for Azure DevOps.
3. Paste a **personal access token**.

The token is checked with the service and kept in the **macOS Keychain**, never in a file or a log. The dialog lists the permissions
the token needs, named as the service names them:

| Service | Token permission |
| --- | --- |
| **GitHub** | Contents and Pull requests, *Read and write* (or the `repo` scope of a classic token) |
| **GitLab** | the `api` scope |
| **Azure DevOps** | Code, *Read & write*, with the token's organization set to **yours**, not "All accessible organizations" (that choice stops working on December 1, 2026) |

After connecting, gcode reports what it could confirm:

- GitLab and classic GitHub tokens list their scopes, so a missing one is named.
- For a fine-grained GitHub token and for Azure DevOps, only *reading* can be tried. An information notice says that pushing and pull
  requests were **not checked**; a warning appears only for a permission that is certainly missing.

## What an account is used for

| Use | Detail |
| --- | --- |
| Browse repositories | **Add project → Clone → Browse my repositories…** |
| Pull requests | List the open ones in the branch menu and create one (**Create pull request…**) |
| Git over HTTPS | `clone`, `fetch`, `pull` and `push` get the token for that host. **SSH remotes keep using your keys.** |
| **View on …** | **Repository → View on GitHub / GitLab / Azure DevOps** (`⇧⌘G`) finds the service from the remote's address, or from an account for a self-hosted host |

```text
 push / pull / fetch / clone (HTTPS)
   1. token of the connected account for that host   ← tried first
   2. refused (HTTP 401/403)?  → your own Git login (credential helper / keychain)
   3. both refused             → one message that names both
```

## When a token is refused

A push, pull, fetch or pull request that the host turns down, or a permission found missing while connecting, marks the account. A small
**warning** appears on the Settings button in the activity bar. It opens **Settings → Accounts**, where the account shows what is missing
and which permissions to give the token, with a button that opens the service's token page.

The warning stays until it is fixed. After changing the token's permissions (or connecting again with a new token) press **Check
again**: it asks the service what the token may do — it can start a push and open a pull request, without changing anything — in the open
project's repository, else in the account's first repository.

**Disconnect** removes the account and its token from the Keychain.

## Privacy

- Tokens are passed to Git through environment variables read by a one-command credential helper, so they appear in no command line,
  file or log.
- Accounts belong to a [profile](./projects.md#profiles); another profile cannot see them.
