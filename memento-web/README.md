# Memento Web

Product-facing web surface for Memento.

This app should explain the product clearly and eventually become the local memory UI. It is not the architectural source of truth; the core remains `libmemento` + `mementod` + `memento-cli`.

## Local Development

Install dependencies and start the dev server:

```bash
npm install
npm run dev
```

Build and lint before shipping:

```bash
npm run lint
npm run build
```

## Product Constraints

- Do not reintroduce generic scaffold copy.
- Keep the language specific to local-first memory for humans and agents.
- Prefer pages that explain the actual engine and workflow over marketing fluff.
- If the UI starts diverging from the current product direction, `VISION.md` wins.
