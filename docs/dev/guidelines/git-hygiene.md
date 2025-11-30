# Git Hygiene Setup

## 1. Commit Signing (Mandatory)

All commits must be signed to be accepted.

1. **Generate a Key** (if you don't have one):

    ```bash
    gpg --full-generate-key
    ```

2. **Get Key ID**:

    ```bash
    gpg --list-secret-keys --keyid-format=long
    # Look for 'sec' line, e.g., sec   rsa4096/3AA5C34371567BD2
    ```

3. **Configure Git**:

    ```bash
    git config --global user.signingkey <YOUR_KEY_ID>
    git config --global commit.gpgsign true
    ```

## 2. Commit Message Format

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```text
type(scope): subject

body

footer
```

- **Types**: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`, `ci`.
- **Example**:

    ```text
    feat(auth): add JWT validation middleware

    Implements stricter validation for bearer tokens to prevent replay attacks.

    Fixes #123
    ```

## 3. Tooling Setup

This repo uses `husky` and `commitlint` to enforce these rules locally.

```bash
# Install tooling
npm install

# Enable hooks
npm run prepare
```
