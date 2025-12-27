import { useAppStore } from '../../../store';

interface TickerItemProps {
  ticker: string;
  isSelected: boolean;
  isCached: boolean;
  isFocused: boolean;
  onToggle: () => void;
}

function TickerItem({
  ticker,
  isSelected,
  isCached,
  isFocused,
  onToggle,
}: TickerItemProps) {
  return (
    <div
      className={`ticker-item ${isFocused ? 'focused' : ''}`}
      onClick={onToggle}
    >
      <span className="ticker-indicator">{isFocused ? '▸' : ' '}</span>
      <span className={`ticker-checkbox ${isSelected ? 'checked' : ''}`}>
        [{isSelected ? '✓' : ' '}]
      </span>
      <span className="ticker-symbol">{ticker}</span>
      <span className={`ticker-cache ${isCached ? 'cached' : ''}`}>
        {isCached ? '●' : '○'}
      </span>
    </div>
  );
}

export function TickerList() {
  const universe = useAppStore((s) => s.universe);
  const selectedSectorIndex = useAppStore((s) => s.selectedSectorIndex);
  const selectedTickerIndex = useAppStore((s) => s.selectedTickerIndex);
  const selectedTickers = useAppStore((s) => s.selectedTickers);
  const cachedSymbols = useAppStore((s) => s.cachedSymbols);
  const toggleTicker = useAppStore((s) => s.toggleTicker);
  const collapseToSectors = useAppStore((s) => s.collapseToSectors);

  const sector = universe?.sectors[selectedSectorIndex];
  const tickers = sector?.tickers ?? [];

  if (!sector) {
    return (
      <div className="ticker-list empty">
        <p>No sector selected</p>
      </div>
    );
  }

  return (
    <div className="ticker-list">
      <div className="ticker-list-header">
        <button className="back-button" onClick={collapseToSectors}>
          ←
        </button>
        <span className="sector-title">{sector.name}</span>
        <span className="ticker-count">{tickers.length} tickers</span>
      </div>
      <div className="ticker-items">
        {tickers.map((ticker, index) => (
          <TickerItem
            key={ticker}
            ticker={ticker}
            isSelected={selectedTickers.has(ticker)}
            isCached={cachedSymbols.has(ticker)}
            isFocused={index === selectedTickerIndex}
            onToggle={() => toggleTicker(ticker)}
          />
        ))}
      </div>

      <style>{`
        .ticker-list {
          display: flex;
          flex-direction: column;
          height: 100%;
        }
        .ticker-list.empty {
          display: flex;
          align-items: center;
          justify-content: center;
          color: var(--muted);
        }
        .ticker-list-header {
          display: flex;
          align-items: center;
          gap: var(--space-sm);
          padding: var(--space-sm) var(--space-md);
          border-bottom: 1px solid var(--border);
          background: var(--bg-secondary);
        }
        .back-button {
          background: none;
          border: 1px solid var(--border);
          color: var(--muted);
          padding: var(--space-xs) var(--space-sm);
          border-radius: var(--radius-sm);
          cursor: pointer;
          font-family: var(--font-mono);
        }
        .back-button:hover {
          background: var(--bg);
          color: var(--fg);
        }
        .sector-title {
          flex: 1;
          font-weight: 600;
          color: var(--fg);
        }
        .ticker-count {
          color: var(--muted);
          font-size: var(--font-size-xs);
        }
        .ticker-items {
          flex: 1;
          overflow-y: auto;
        }
        .ticker-item {
          display: flex;
          align-items: center;
          gap: var(--space-sm);
          padding: var(--space-xs) var(--space-md);
          cursor: pointer;
          font-family: var(--font-mono);
          font-size: var(--font-size-sm);
        }
        .ticker-item:hover {
          background: var(--bg-secondary);
        }
        .ticker-item.focused {
          background: var(--blue-bg);
        }
        .ticker-indicator {
          color: var(--yellow);
          width: 1ch;
        }
        .ticker-checkbox {
          color: var(--muted);
        }
        .ticker-checkbox.checked {
          color: var(--green);
        }
        .ticker-symbol {
          flex: 1;
          color: var(--cyan);
        }
        .ticker-cache {
          color: var(--muted);
        }
        .ticker-cache.cached {
          color: var(--green);
        }
      `}</style>
    </div>
  );
}
