import { useAppStore } from '../../../store';
import type { Sector } from '../../../types';

interface SectorItemProps {
  sector: Sector;
  isSelected: boolean;
  selectedCount: number;
}

function SectorItem({ sector, isSelected, selectedCount }: SectorItemProps) {
  const setViewMode = useAppStore((s) => s.setViewMode);
  const { selectedSectorIndex } = useAppStore();
  const universe = useAppStore((s) => s.universe);
  const sectorIndex = universe?.sectors.findIndex((s) => s.id === sector.id) ?? -1;
  const navigateSector = useAppStore((s) => s.navigateSector);

  const handleClick = () => {
    // Navigate to this sector and expand to tickers
    if (sectorIndex !== selectedSectorIndex) {
      navigateSector(sectorIndex - selectedSectorIndex);
    }
    setViewMode('tickers');
  };

  const totalCount = sector.tickers.length;
  const hasSelection = selectedCount > 0;

  return (
    <div
      className={`sector-item ${isSelected ? 'selected' : ''}`}
      onClick={handleClick}
    >
      <span className="sector-indicator">{isSelected ? '▸' : ' '}</span>
      <span className="sector-name">{sector.name}</span>
      <span className={`sector-count ${hasSelection ? 'has-selection' : ''}`}>
        [{selectedCount}/{totalCount}]
      </span>
    </div>
  );
}

export function SectorList() {
  const universe = useAppStore((s) => s.universe);
  const selectedSectorIndex = useAppStore((s) => s.selectedSectorIndex);
  const getSelectedCountForSector = useAppStore((s) => s.getSelectedCountForSector);

  if (!universe) {
    return (
      <div className="sector-list loading">
        <p>Loading universe...</p>
      </div>
    );
  }

  return (
    <div className="sector-list">
      <div className="sector-list-header">Sectors</div>
      <div className="sector-items">
        {universe.sectors.map((sector, index) => (
          <SectorItem
            key={sector.id}
            sector={sector}
            isSelected={index === selectedSectorIndex}
            selectedCount={getSelectedCountForSector(sector.id)}
          />
        ))}
      </div>

      <style>{`
        .sector-list {
          display: flex;
          flex-direction: column;
          height: 100%;
          border-right: 1px solid var(--border);
        }
        .sector-list.loading {
          display: flex;
          align-items: center;
          justify-content: center;
          color: var(--muted);
        }
        .sector-list-header {
          padding: var(--space-sm) var(--space-md);
          font-weight: 600;
          color: var(--fg);
          border-bottom: 1px solid var(--border);
          background: var(--bg-secondary);
        }
        .sector-items {
          flex: 1;
          overflow-y: auto;
        }
        .sector-item {
          display: flex;
          align-items: center;
          gap: var(--space-sm);
          padding: var(--space-xs) var(--space-md);
          cursor: pointer;
          font-family: var(--font-mono);
          font-size: var(--font-size-sm);
        }
        .sector-item:hover {
          background: var(--bg-secondary);
        }
        .sector-item.selected {
          background: var(--blue-bg);
          color: var(--blue);
          font-weight: 600;
        }
        .sector-indicator {
          color: var(--yellow);
          width: 1ch;
        }
        .sector-name {
          flex: 1;
        }
        .sector-count {
          color: var(--muted);
          font-size: var(--font-size-xs);
        }
        .sector-count.has-selection {
          color: var(--green);
        }
      `}</style>
    </div>
  );
}
