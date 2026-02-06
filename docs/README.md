# Documentation Site

This directory contains the Docusaurus site for Zelana.

## Requirements

- Node.js >= 20 (see `package.json` engines)
- Yarn classic (v1) or npm

## Install

```bash
yarn install
```

## Local Development

```bash
yarn start
```

This starts the dev server with hot reload.

## Build

```bash
yarn build
```

Build output goes to `build/`.

## Serve a Build

```bash
yarn serve
```

## Deployment

Using SSH:

```bash
USE_SSH=true yarn deploy
```

Not using SSH:

```bash
GIT_USER=<your GitHub username> yarn deploy
```

If you use GitHub Pages for hosting, this command builds the site and pushes to the
`gh-pages` branch.
