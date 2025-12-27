# TrendLab GUI (Tauri + React)

This app is a **GUI shell** around `trendlab-core`.

- Rust (`src-tauri/`) is authoritative for all domain logic.
- TypeScript (`ui/`) handles layout, interaction, and visualization only.

## Dev

### Rust

From repo root:

```bash
cargo check -p trendlab-gui
```

### UI

From `apps/trendlab-gui/ui/`:

```bash
npm install
npm run dev
```


