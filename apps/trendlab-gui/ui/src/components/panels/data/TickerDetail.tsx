import { useAppStore } from '../../../store';

export function TickerDetail() {
  const universe = useAppStore((s) => s.universe);
  const selectedSectorIndex = useAppStore((s) => s.selectedSectorIndex);
  const selectedTickerIndex = useAppStore((s) => s.selectedTickerIndex);
  const selectedTickers = useAppStore((s) => s.selectedTickers);
  const cachedSymbols = useAppStore((s) => s.cachedSymbols);
  const viewMode = useAppStore((s) => s.viewMode);

  const sector = universe?.sectors[selectedSectorIndex];
  const tickers = sector?.tickers ?? [];
  const currentTicker = tickers[selectedTickerIndex];

  if (viewMode === 'sectors' || !currentTicker) {
    // Show sector summary
    const totalSelected = selectedTickers.size;
    const totalCached = [...selectedTickers].filter((t) =>
      cachedSymbols.has(t)
    ).length;

    return (
      <div className="ticker-detail">
        <div className="detail-row">
          <span className="detail-label">Selected:</span>
          <span className="detail-value">{totalSelected} tickers</span>
        </div>
        <div className="detail-row">
          <span className="detail-label">Cached:</span>
          <span className="detail-value">{totalCached} of {totalSelected}</span>
        </div>
        <div className="detail-shortcuts">
          <span>Space: Toggle</span>
          <span>a: All</span>
          <span>n: None</span>
          <span>s: Search</span>
          <span>f: Fetch</span>
        </div>

        <style>{`
          .ticker-detail {
            display: flex;
            flex-direction: column;
            gap: var(--space-sm);
            padding: var(--space-md);
            border-top: 1px solid var(--border);
            background: var(--bg-secondary);
            font-size: var(--font-size-sm);
          }
          .detail-row {
            display: flex;
            gap: var(--space-sm);
          }
          .detail-label {
            color: var(--muted);
          }
          .detail-value {
            color: var(--fg);
          }
          .detail-shortcuts {
            display: flex;
            gap: var(--space-md);
            margin-top: var(--space-sm);
            padding-top: var(--space-sm);
            border-top: 1px solid var(--border);
            color: var(--muted);
            font-family: var(--font-mono);
            font-size: var(--font-size-xs);
          }
        `}</style>
      </div>
    );
  }

  const isSelected = selectedTickers.has(currentTicker);
  const isCached = cachedSymbols.has(currentTicker);

  return (
    <div className="ticker-detail">
      <div className="detail-row">
        <span className="detail-label">Ticker:</span>
        <span className="detail-ticker">{currentTicker}</span>
        {isSelected && <span className="detail-selected">✓ selected</span>}
      </div>
      <div className="detail-row">
        <span className="detail-label">Status:</span>
        <span className={`detail-status ${isCached ? 'cached' : 'not-cached'}`}>
          {isCached ? '● Data cached' : '○ Data not loaded'}
        </span>
      </div>
      <div className="detail-shortcuts">
        <span>Space: Toggle</span>
        <span>a: All</span>
        <span>n: None</span>
        <span>←: Sectors</span>
        <span>f: Fetch</span>
      </div>

      <style>{`
        .ticker-detail {
          display: flex;
          flex-direction: column;
          gap: var(--space-sm);
          padding: var(--space-md);
          border-top: 1px solid var(--border);
          background: var(--bg-secondary);
          font-size: var(--font-size-sm);
        }
        .detail-row {
          display: flex;
          gap: var(--space-sm);
          align-items: center;
        }
        .detail-label {
          color: var(--muted);
        }
        .detail-ticker {
          color: var(--cyan);
          font-weight: 600;
          font-family: var(--font-mono);
        }
        .detail-selected {
          color: var(--green);
          font-size: var(--font-size-xs);
        }
        .detail-status {
          font-family: var(--font-mono);
        }
        .detail-status.cached {
          color: var(--green);
        }
        .detail-status.not-cached {
          color: var(--muted);
        }
        .detail-shortcuts {
          display: flex;
          gap: var(--space-md);
          margin-top: var(--space-sm);
          padding-top: var(--space-sm);
          border-top: 1px solid var(--border);
          color: var(--muted);
          font-family: var(--font-mono);
          font-size: var(--font-size-xs);
        }
      `}</style>
    </div>
  );
}
